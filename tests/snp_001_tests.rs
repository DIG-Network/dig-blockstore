//! # SNP-001 — Export snapshot: canonical blocks with manifest and SHA-256 checksum
//!
//! **Trace**
//! - Spec: [`SNP-001.md`](../docs/requirements/domains/snapshot/specs/SNP-001.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, SNAPSHOT_VERSION};

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_export_snapshot_basic() {
    // **Proves:** SNP-001 AC — writes manifest + blocks + checksum.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    let manifest = store.export_snapshot(0, 9, &mut buf).expect("export");

    assert_eq!(manifest.version, SNAPSHOT_VERSION);
    assert_eq!(manifest.start_height, 0);
    assert_eq!(manifest.end_height, 9);
    assert_eq!(manifest.block_count, 10);
    assert!(!buf.is_empty(), "snapshot should contain data");
}

#[test]
fn test_export_import_round_trip() {
    // **Proves:** SNP-001 + SNP-002 — export then import into a fresh store.
    let (_guard1, path1) = temp_blockstore_dir();
    let store1 = BlockStore::open(test_config(path1)).expect("open source");
    let chain = build_chain(5);
    for block in &chain {
        store1.extend_chain(block).expect("extend");
    }

    // Export
    let mut buf = Vec::new();
    let manifest = store1.export_snapshot(0, 4, &mut buf).expect("export");

    // Import into fresh store
    let (_guard2, path2) = temp_blockstore_dir();
    let store2 = BlockStore::open(test_config(path2)).expect("open dest");

    let mut cursor = std::io::Cursor::new(&buf);
    let imported = store2.import_snapshot(&mut cursor).expect("import");
    assert_eq!(imported.version, manifest.version);
    assert_eq!(imported.start_height, 0);
    assert_eq!(imported.end_height, 4);

    // Verify all blocks exist in the destination store
    for block in &chain {
        let got = store2
            .get_block(&block.hash())
            .expect("get")
            .expect("should exist after import");
        assert_eq!(got.hash(), block.hash());
    }
}

#[test]
fn test_export_partial_range() {
    // **Proves:** SNP-001 — can export a subrange [3, 7] of a longer chain.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let mut buf = Vec::new();
    let manifest = store.export_snapshot(3, 7, &mut buf).expect("export");
    assert_eq!(manifest.start_height, 3);
    assert_eq!(manifest.end_height, 7);
    assert_eq!(manifest.block_count, 5);
}
