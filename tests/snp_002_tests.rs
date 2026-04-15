//! # SNP-002 — Import snapshot: validate manifest, contiguity, parent links, checksum
//!
//! **Trace**
//! - Spec: [`SNP-002.md`](../docs/requirements/domains/snapshot/specs/SNP-002.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, BlockStoreError};

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_import_snapshot_validates_checksum() {
    // **Proves:** SNP-002 AC §7/§8 — checksum mismatch returns Serialization error.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    store1.export_snapshot(0, 4, &mut buf).expect("export");

    // Corrupt the last byte (part of checksum)
    let last = buf.len() - 1;
    buf[last] ^= 0xFF;

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    let err = store2
        .import_snapshot(&mut cursor)
        .expect_err("should fail");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref msg) if msg.contains("checksum")),
        "expected checksum error, got {err:?}"
    );
}

#[test]
fn test_import_snapshot_rejects_bad_version() {
    // **Proves:** SNP-002 AC §2 — unsupported version rejected.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    store1.export_snapshot(0, 2, &mut buf).expect("export");

    // Corrupt the version field (first 4 bytes of bincode manifest)
    // bincode uses little-endian for u32, so writing version=99
    buf[0] = 99;
    buf[1] = 0;
    buf[2] = 0;
    buf[3] = 0;

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    let err = store2
        .import_snapshot(&mut cursor)
        .expect_err("should fail");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref msg) if msg.contains("version")),
        "expected version error, got {err:?}"
    );
}

#[test]
fn test_import_snapshot_blocks_stored_canonically() {
    // **Proves:** SNP-002 AC §5/§6 — blocks stored as canonical after import.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    store1.export_snapshot(0, 4, &mut buf).expect("export");

    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open");
    let mut cursor = std::io::Cursor::new(&buf);
    store2.import_snapshot(&mut cursor).expect("import");

    // Blocks should be canonical in the destination
    for (i, block) in chain.iter().enumerate() {
        let hash = store2
            .get_hash_by_height(i as u64)
            .expect("h")
            .expect("canonical");
        assert_eq!(hash, block.hash(), "height {i} should be canonical");
    }
}
