//! # BLK-013 — Maintenance: [`BlockStore::flush`] and [`BlockStore::compact`]
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-013.md`](../docs/requirements/domains/block_storage/specs/BLK-013.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-013)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-013-flush-and-compact)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | 1 | [`flush`](dig_blockstore::BlockStore::flush) completes without error after writes | [`test_flush_succeeds_after_put_block`] |
//! | 2 | [`compact`](dig_blockstore::BlockStore::compact) completes without error after writes | [`test_compact_succeeds_after_genesis`] |
//! | 3 | RocksDB failures surface as [`BlockStoreError::RocksDb`] | Covered by `?` in implementation ([`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md)); no deterministic failure injection here |
//! | 4 | Neither API changes logical block / tip / record state | [`test_flush_then_compact_preserves_get_block_and_stats_counts`] |
//!
//! **Durability test plan:** [`test_flush_data_survives_store_drop_and_reopen`] — after [`flush`], drop the in-process
//! [`BlockStore`] (releases the RocksDB lock), reopen from the same path, and prove the block is still readable
//! ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) read path).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// **Proves:** BLK-013 test plan `test_flush_succeeds` — [`BlockStore::flush`] returns `Ok(())` after a successful
/// [`BlockStore::put_block`] ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md)).
#[test]
fn test_flush_succeeds_after_put_block() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    let b = test_block(1, genesis.hash());
    store.put_block(&b, true).expect("put");
    store.flush().expect("flush must succeed");
}

/// **Proves:** BLK-013 test plan `test_compact_succeeds` — [`BlockStore::compact`] returns `Ok(())` on a populated DB.
#[test]
fn test_compact_succeeds_after_genesis() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    store.compact().expect("compact must succeed");
}

/// **Proves:** BLK-013 test plan `test_flush_data_durable` — after [`flush`], closing the store and reopening the
/// same directory must still yield the same block bytes via [`BlockStore::get_block`].
#[test]
fn test_flush_data_survives_store_drop_and_reopen() {
    let (_guard, path) = temp_blockstore_dir();
    let cfg = test_config(path.clone());
    let hash = {
        let store = BlockStore::open(cfg.clone()).expect("open");
        let genesis = build_chain(1).into_iter().next().expect("genesis");
        store.init_genesis(&genesis).expect("init");
        let b = test_block(1, genesis.hash());
        let h = b.hash();
        store.put_block(&b, true).expect("put");
        store.flush().expect("flush");
        h
    };
    let store2 = BlockStore::open(cfg).expect("reopen");
    let round = store2.get_block(&hash).expect("get").expect("still there");
    assert_eq!(round.hash(), hash);
}

/// **Proves:** AC 4 — capture [`BlockStore::get_record`] + logical CF counts from [`BlockStore::stats`] before and
/// after [`flush`] then [`compact`]; hashes, record fields, and per-CF **key counts** stay identical. We compare
/// counts only (not `total_size_bytes`) because SST layout may change the live-size estimate after compaction
/// ([`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md)).
#[test]
fn test_flush_then_compact_preserves_get_block_and_stats_counts() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = build_chain(1).into_iter().next().expect("genesis");
    store.init_genesis(&genesis).expect("init");
    let b = test_block(1, genesis.hash());
    let h = b.hash();
    store.put_block(&b, true).expect("put");
    let rec_before = store.get_record(&h).expect("record").expect("cached");
    let stats_before = store.stats().expect("stats");

    store.flush().expect("flush");
    store.compact().expect("compact");

    let rec_after = store.get_record(&h).expect("record").expect("cached");
    assert_eq!(
        rec_before, rec_after,
        "flush/compact must not touch BlockRecord cache semantics"
    );
    let got = store.get_block(&h).expect("get").expect("block");
    assert_eq!(got.hash(), h);
    let stats_after = store.stats().expect("stats");
    assert_eq!(stats_before.block_count, stats_after.block_count);
    assert_eq!(
        stats_before.canonical_block_count,
        stats_after.canonical_block_count
    );
    assert_eq!(stats_before.header_count, stats_after.header_count);
    assert_eq!(stats_before.checkpoint_count, stats_after.checkpoint_count);
    assert_eq!(stats_before.attested_count, stats_after.attested_count);
    assert_eq!(stats_before.tip_height, stats_after.tip_height);
    assert_eq!(stats_before.min_height, stats_after.min_height);
}
