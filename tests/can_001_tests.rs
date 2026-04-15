//! # CAN-001 — Dual-layer canonical index (`canonical.bin` + [`CF_CANONICAL`])
//!
//! **Trace**
//! - Spec + test plan: [`CAN-001.md`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-001)](../docs/requirements/domains/canonical_chain/NORMATIVE.md#can-001-dual-layer-canonical-index)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What we prove
//!
//! [`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) requires a **recoverable mmap hot path**
//! (`canonical.bin`, dense `height × 32` bytes per [`CAN-002`](../docs/requirements/domains/canonical_chain/specs/CAN-002.md))
//! and a **durable RocksDB cold path** ([`CF_CANONICAL`]). On every [`BlockStore::open`], the sidecar must match
//! [`CF_CANONICAL`] or be rebuilt — never panic on corruption. Height lookups must succeed when mmap is disabled
//! (RocksDB-only), matching the spec’s fallback semantics.
//!
//! Each test below maps to the CAN-001 acceptance criteria / test plan table (`test_dual_layer_*`, `test_mmap_*`).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::height_key;
use dig_blockstore::{BlockStore, CF_CANONICAL};

use common::{build_chain, temp_blockstore_dir, test_config};

/// `canonical.bin` lives next to the RocksDB directory (see `CANONICAL_BIN_FILE` in `src/canonical/mmap.rs`).
fn canonical_bin_path(db: &Path) -> std::path::PathBuf {
    db.join("canonical.bin")
}

fn open_opts_write() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Read the authoritative `height → [u8;32]` mapping from [`CF_CANONICAL`] using a fresh DB handle — independent of
/// [`BlockStore`]'s mmap layer so we can compare on-disk bytes to RocksDB after reopen/rebuild.
fn cf_canonical_hash_at(db_path: &Path, height: u64) -> Option<[u8; 32]> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors(&open_opts_write(), db_path, cfs)
        .expect("open cf for probe");
    let cf = db.cf_handle(CF_CANONICAL).expect("cf canonical");
    let v = db
        .get_cf(cf, height_key(height).as_slice())
        .expect("get_cf")?;
    let s: &[u8] = v.as_ref();
    <[u8; 32]>::try_from(s).ok()
}

/// **Proves:** CAN-001 AC “creates both canonical.bin and CF_CANONICAL” + test plan `test_dual_layer_both_written` —
/// after writing a short canonical chain, every height’s hash in RocksDB matches the same 32-byte slice inside
/// `canonical.bin` at offset `height * 32`.
#[test]
fn test_dual_layer_both_written() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let chain = build_chain(5);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put canonical"));
    }
    drop(store);

    let bin = std::fs::read(canonical_bin_path(&path)).expect("read canonical.bin");
    assert_eq!(bin.len(), 5 * 32, "dense file covers heights 0..=4");
    for h in 0u64..5 {
        let got_cf = cf_canonical_hash_at(&path, h).expect("cf row");
        let sl: &[u8] = &bin[(h as usize * 32)..(h as usize * 32 + 32)];
        let got_mmap: [u8; 32] = <[u8; 32]>::try_from(sl).expect("32-byte window");
        assert_eq!(
            got_cf, got_mmap,
            "height {h}: mmap bytes must match CF_CANONICAL"
        );
    }
}

/// **Proves:** CAN-001 AC “deleting canonical.bin and reopening triggers automatic rebuild” + test plan
/// `test_mmap_rebuild_on_missing` — the mmap sidecar is recreated from [`CF_CANONICAL`] so lookups still work and
/// bytes match CF exactly.
#[test]
fn test_mmap_rebuild_on_missing() {
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        let chain = build_chain(4);
        for b in &chain {
            assert!(store.put_block(b, true).expect("put"));
        }
    }
    std::fs::remove_file(canonical_bin_path(&path)).expect("delete canonical.bin");

    let store = BlockStore::open(test_config(path.clone())).expect("reopen");
    let b = store
        .get_block_by_height(2)
        .expect("query")
        .expect("block at 2");
    assert_eq!(b.height(), 2);
    // Release RocksDB `LOCK` before opening a second `DB` handle for [`cf_canonical_hash_at`] (Windows).
    drop(store);

    let bin = std::fs::read(canonical_bin_path(&path)).expect("rebuilt file");
    assert_eq!(bin.len(), 4 * 32);
    for h in 0u64..4 {
        let sl: &[u8] = &bin[(h as usize * 32)..(h as usize * 32 + 32)];
        let mmap_h: [u8; 32] = <[u8; 32]>::try_from(sl).expect("32-byte window");
        assert_eq!(cf_canonical_hash_at(&path, h).expect("cf"), mmap_h);
    }
}

/// **Proves:** CAN-001 AC “corrupt canonical.bin triggers rebuild, not a panic” + test plan `test_mmap_rebuild_on_corrupt`.
/// We truncate the sidecar to an invalid length (not a multiple of 32), reopen, and assert normal queries plus a
/// repaired dense layout.
#[test]
fn test_mmap_rebuild_on_corrupt() {
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        let chain = build_chain(3);
        for b in &chain {
            assert!(store.put_block(b, true).expect("put"));
        }
    }
    std::fs::write(canonical_bin_path(&path), b"not-a-multiple-of-32!")
        .expect("write corrupt canonical.bin");

    let store = BlockStore::open(test_config(path.clone())).expect("reopen must not panic");
    assert!(store.get_block_by_height(1).expect("get").is_some());

    let bin = std::fs::read(canonical_bin_path(&path)).expect("read after rebuild");
    assert_eq!(bin.len(), 3 * 32);
}

/// **Proves:** CAN-001 AC “height lookups fall back to CF_CANONICAL when mmap is unavailable” + test plan
/// `test_mmap_fallback_to_rocksdb` — [`BlockStore::disable_canonical_bin_acceleration`] forces the RocksDB path while
/// leaving [`CF_CANONICAL`] intact; [`get_block_by_height`](dig_blockstore::BlockStore::get_block_by_height) must still
/// return the same block.
#[test]
fn test_mmap_fallback_to_rocksdb() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let chain = build_chain(6);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    store.disable_canonical_bin_acceleration();
    let b = store
        .get_block_by_height(4)
        .expect("lookup")
        .expect("block");
    assert_eq!(b.height(), 4);
}

/// **Proves:** CAN-001 test plan `test_mmap_vs_rocksdb_consistency` at integration scale — we use **200** canonical
/// heights (the normative example cites 1000; 200 keeps CI fast while still exercising growth/remap and dense layout).
/// For every height, the mmap file slice and [`CF_CANONICAL`] row are identical.
#[test]
fn test_mmap_vs_rocksdb_consistency() {
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        let chain = build_chain(200);
        for b in &chain {
            assert!(store.put_block(b, true).expect("put"));
        }
    }
    let bin = std::fs::read(canonical_bin_path(&path)).expect("read canonical.bin");
    assert_eq!(bin.len(), 200 * 32);
    for h in 0u64..200 {
        let cf_h = cf_canonical_hash_at(&path, h).expect("cf row");
        let sl: &[u8] = &bin[(h as usize * 32)..(h as usize * 32 + 32)];
        let mmap_h: [u8; 32] = <[u8; 32]>::try_from(sl).expect("32-byte window");
        assert_eq!(cf_h, mmap_h, "mismatch at height {h}");
    }
}
