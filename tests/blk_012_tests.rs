//! # BLK-012 — Storage statistics (`stats` → [`StorageStats`])
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-012.md`](../docs/requirements/domains/block_storage/specs/BLK-012.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-012)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-012-storage-statistics-stats)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//! - Shape of [`StorageStats`]: [`TYP-007.md`](../docs/requirements/domains/storage_types/specs/TYP-007.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | 1 | All eight [`StorageStats`] fields populated | Every test reads a full struct |
//! | 2 | `block_count` includes forks (all [`CF_BLOCKS`] rows) | [`test_stats_block_count_includes_non_canonical_fork`] |
//! | 3 | `canonical_block_count` only [`CF_CANONICAL`] rows | Same test vs `block_count` |
//! | 4 | `tip_height` is [`None`] with no tip, [`Some`] after genesis | [`test_stats_empty_store`], [`test_stats_tip_height_after_genesis`] |
//! | 5 | `min_height` is [`None`] until [`META_MIN_HEIGHT`] exists | [`test_stats_empty_store`], [`test_stats_min_height_after_metadata_write`] |
//! | 6 | `total_size_bytes` is a positive disk estimate after data + flush | [`test_stats_total_size_positive_after_flush`] |
//!
//! **Pruning metadata encoding:** [`storage_types/NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md) — `META_MIN_HEIGHT` value is **8 bytes little-endian** `u64` (same width as height keys, distinct key string per [`KEY-004`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md)).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_block::{AttestedBlock, ReceiptList};
use dig_blockstore::cf_options;
use dig_blockstore::{BlockStore, StorageStats, CF_METADATA, META_MIN_HEIGHT};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// Reopen the database with the **same** column-family descriptors as [`BlockStore::open`] and call [`rocksdb::DB::flush`].
///
/// **Why:** RocksDB’s `rocksdb.estimate-live-data-size` (used inside [`BlockStore::stats`]) often stays at zero until
/// memtables are flushed; BLK-012’s test plan expects a **strict** `total_size_bytes > 0` after storing blocks.
fn flush_db_for_size_property(path: &Path) {
    let cfg = test_config(path.to_path_buf());
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cfs = cf_options::column_family_descriptors(&cfg);
    let db = rocksdb::DB::open_cf_descriptors(&opts, path, cfs).expect("reopen for flush");
    db.flush().expect("rocksdb flush");
}

/// Raw write of [`META_MIN_HEIGHT`] after the [`BlockStore`] is dropped so nothing holds the DB lock.
///
/// **Proves:** NORMATIVE BLK-012 item 8 — `min_height` reflects persisted pruning floor without requiring the full
/// PRN prune implementation in this crate yet ([`PRN-004`](../docs/requirements/domains/pruning/specs/PRN-004_min_retained_height_tracking.md)).
fn write_meta_min_height_raw(path: &Path, height: u64) {
    let cfg = test_config(path.to_path_buf());
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cfs = cf_options::column_family_descriptors(&cfg);
    let db = rocksdb::DB::open_cf_descriptors(&opts, path, cfs).expect("reopen for meta");
    let meta = db.cf_handle(CF_METADATA).expect("metadata cf");
    db.put_cf(meta, META_MIN_HEIGHT.as_bytes(), height.to_le_bytes())
        .expect("META_MIN_HEIGHT put");
    db.flush().expect("flush meta");
}

/// **Proves:** BLK-012 test plan `test_stats_empty_store` — fresh directory, [`BlockStore::open`] only (no
/// [`BlockStore::init_genesis`]): every CF iterator count is zero, no tip, no min retained row, disk estimate zero
/// or unchanged from an empty SST layout.
#[test]
fn test_stats_empty_store() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let s = store.stats().expect("stats");
    assert_eq!(
        s,
        StorageStats {
            block_count: 0,
            canonical_block_count: 0,
            header_count: 0,
            checkpoint_count: 0,
            attested_count: 0,
            tip_height: None,
            min_height: None,
            total_size_bytes: 0,
        }
    );
}

/// **Proves:** AC 4 — after [`BlockStore::init_genesis`], [`StorageStats::tip_height`] matches the in-memory tip.
#[test]
fn test_stats_tip_height_after_genesis() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init_genesis");
    let s = store.stats().expect("stats");
    assert_eq!(s.tip_height, Some(0));
    assert_eq!(s.block_count, 1);
    assert_eq!(s.header_count, 1);
    assert_eq!(s.canonical_block_count, 1);
}

/// **Proves:** BLK-012 test plan `test_stats_after_blocks` — `N` canonical inserts increment block/header/canonical
/// counts together; [`put_attestation`] bumps [`StorageStats::attested_count`].
///
/// **Tip height:** [`BlockStore::tip`] is only advanced by [`BlockStore::init_genesis`] today ([`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md) is still open); additional [`put_block`] calls do not move the tip in metadata, so [`StorageStats::tip_height`] stays `Some(0)` here while row counts grow.
#[test]
fn test_stats_after_blocks_and_attestation() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(4);
    store.init_genesis(&chain[0]).expect("init");
    store.put_block(&chain[1], true).expect("put 1");
    store.put_block(&chain[2], true).expect("put 2");
    store.put_block(&chain[3], true).expect("put 3");
    let hash = chain[2].hash();
    let attested = AttestedBlock::new(chain[2].clone(), 1, ReceiptList::default());
    store.put_attestation(&hash, &attested).expect("attest");
    let s = store.stats().expect("stats");
    assert_eq!(s.block_count, 4, "four distinct block bodies");
    assert_eq!(s.header_count, 4);
    assert_eq!(s.canonical_block_count, 4);
    assert_eq!(s.attested_count, 1);
    assert_eq!(s.checkpoint_count, 0);
    assert_eq!(s.tip_height, Some(0));
}

/// **Proves:** AC 2 vs 3 — an off-chain [`put_block`] with `canonical = false` increases `block_count` and
/// `header_count` but **not** [`StorageStats::canonical_block_count`].
#[test]
fn test_stats_block_count_includes_non_canonical_fork() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    let orphan = test_block(99, genesis.hash());
    store
        .put_block(&orphan, false)
        .expect("orphan not canonical");
    let s = store.stats().expect("stats");
    assert_eq!(s.canonical_block_count, 1);
    assert_eq!(s.block_count, 2);
    assert_eq!(s.header_count, 2);
}

/// **Proves:** AC 5 + NORMATIVE 8 — external write of [`META_MIN_HEIGHT`] is surfaced as [`StorageStats::min_height`].
#[test]
fn test_stats_min_height_after_metadata_write() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    drop(store);
    write_meta_min_height_raw(&path, 42);
    let store = BlockStore::open(test_config(path)).expect("reopen");
    let s = store.stats().expect("stats");
    assert_eq!(s.min_height, Some(42));
}

/// **Proves:** BLK-012 test plan `test_stats_total_size` — after persisted rows and a shared RocksDB [`flush`],
/// [`StorageStats::total_size_bytes`] is strictly positive (live-data estimate becomes visible).
#[test]
fn test_stats_total_size_positive_after_flush() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    drop(store);
    flush_db_for_size_property(&path);
    let store = BlockStore::open(test_config(path)).expect("reopen");
    let s = store.stats().expect("stats");
    assert!(
        s.total_size_bytes > 0,
        "estimate-live-data-size sum should be > 0 after flush with genesis data"
    );
}
