//! # SNP-004 — Checksum verification: SHA-256 of all preceding bytes
//!
//! **Trace**
//! - Spec: [`SNP-004.md`](../docs/requirements/domains/snapshot/specs/SNP-004.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, BlockStoreError};

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_valid_snapshot_passes_checksum() {
    // **Proves:** SNP-004 — uncorrupted snapshot passes checksum verification.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    let manifest = store1.export_snapshot(0, 4, &mut buf).expect("export");

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    let imported = store2
        .import_snapshot(&mut cursor)
        .expect("import should succeed");
    assert_eq!(imported.start_height, manifest.start_height);
}

#[test]
fn test_single_byte_corruption_detected() {
    // **Proves:** SNP-004 — single byte flip in block data detected by checksum.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    store1.export_snapshot(0, 2, &mut buf).expect("export");

    // Corrupt a byte in the middle of the block data (not the checksum itself)
    let mid = buf.len() / 2;
    buf[mid] ^= 0x01;

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    // Should fail with either a checksum mismatch or a deserialization error
    // (the corrupted byte might break zstd/bincode before reaching checksum check)
    assert!(
        store2.import_snapshot(&mut cursor).is_err(),
        "corrupted snapshot must fail"
    );
}

#[test]
fn test_truncated_snapshot_detected() {
    // **Proves:** SNP-004 — truncated stream (missing checksum) detected.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    store1.export_snapshot(0, 2, &mut buf).expect("export");

    // Remove the trailing checksum (last 32 bytes)
    buf.truncate(buf.len().saturating_sub(32));

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    assert!(
        store2.import_snapshot(&mut cursor).is_err(),
        "truncated snapshot must fail"
    );
}

#[test]
fn test_checksum_is_deterministic() {
    // **Proves:** SNP-004 — same blocks produce same checksum.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let mut buf1 = Vec::new();
    let m1 = store.export_snapshot(0, 4, &mut buf1).expect("export1");
    let mut buf2 = Vec::new();
    let m2 = store.export_snapshot(0, 4, &mut buf2).expect("export2");

    assert_eq!(m1.checksum, m2.checksum, "same data → same checksum");
    assert_eq!(buf1, buf2, "same data → same bytes");
}
