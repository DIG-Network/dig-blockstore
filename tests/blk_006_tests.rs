//! # BLK-006 — `stream_blocks_in_range`: canonical readahead + sequential block loads
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-006.md`](../docs/requirements/domains/block_storage/specs/BLK-006.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-006)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-006-prefetching-for-sequential-access-stream_blocks_in_range)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//! - Key encoding (height order): [`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)
//!
//! ## Proof strategy (maps to BLK-006 acceptance criteria)
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Heights `start..=end` ascending | [`test_stream_yields_heights_10_through_50_in_order`] |
//! | §2–§3 | Canonical + block paths use readahead-sized [`ReadOptions`](https://docs.rs/rocksdb/latest/rocksdb/ReadOptions.html) | Documented in [`BlockStore::stream_blocks_in_range`](dig_blockstore::BlockStore::stream_blocks_in_range) + integration ordering |
//! | §4 | `readahead_size` from [`BlockStoreConfig`](dig_blockstore::BlockStoreConfig) at open | [`test_readahead_size_exposed_from_store`] |
//! | §5 | Warm block cache ⇒ no extra stream `get_cf_opt` | [`test_second_stream_hits_block_cache_no_extra_stream_reads`] |
//! | §6 | Missing `CF_BLOCKS` row ⇒ [`BlockStoreError::BlockNotFound`](dig_blockstore::BlockStoreError::BlockNotFound) | [`test_missing_cf_blocks_row_yields_block_not_found`] |
//!
//! **Optional throughput check:** [`test_sequential_stream_vs_random_get_block_throughput`] is `#[ignore]` — run with
//! `cargo test --test blk_006_tests test_sequential_stream_vs_random_get_block_throughput -- --ignored` when profiling.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;
use std::time::Instant;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::L2Block;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{BlockStore, BlockStoreError, StreamBlocksInRange, CF_BLOCKS};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

fn open_opts_write() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Delete a single `CF_BLOCKS` value while leaving [`CF_CANONICAL`] intact — proves BLK-006 §6 surfaces
/// [`BlockStoreError::BlockNotFound`](dig_blockstore::BlockStoreError::BlockNotFound) instead of panicking.
fn delete_cf_blocks_row(path: &Path, hash: &Bytes32) {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors(&open_opts_write(), path, cfs)
        .expect("open for maintenance");
    let cf = db.cf_handle(CF_BLOCKS).expect("cf blocks");
    db.delete_cf(cf, hash_key(hash).as_slice())
        .expect("delete_cf blocks row");
}

#[test]
fn test_stream_yields_heights_10_through_50_in_order() {
    // **Proves:** AC §1 — dense canonical chain `0..100`, stream `[10, 50]` must yield **41** blocks with
    // [`L2Block::height`](dig_block::L2Block::height) strictly ascending from 10 to 50 ([`KEY-002`](key_002_tests.rs) big-endian order in `CF_CANONICAL`).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let blocks = build_chain(100);
    for b in &blocks {
        assert!(store.put_block(b, true).expect("put canonical"));
    }
    let collected: Vec<L2Block> = store
        .stream_blocks_in_range(10, 50)
        .expect("stream build")
        .map(|r| r.expect("block ok"))
        .collect();
    assert_eq!(collected.len(), 41);
    for (i, b) in collected.iter().enumerate() {
        assert_eq!(b.height(), 10 + i as u64);
    }
}

#[test]
fn test_readahead_size_exposed_from_store() {
    // **Proves:** AC §4 — the value opened from [`BlockStoreConfig::readahead_size`](dig_blockstore::BlockStoreConfig::readahead_size)
    // is retained on [`BlockStore`](dig_blockstore::BlockStore) and returned by [`readahead_size`](dig_blockstore::BlockStore::readahead_size)
    // so operators/tests can confirm tuning ([`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md) field wiring).
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path);
    cfg.readahead_size = 333_222;
    let store = BlockStore::open(cfg).expect("open");
    assert_eq!(store.readahead_size(), 333_222);
}

#[test]
fn test_second_stream_hits_block_cache_no_extra_stream_reads() {
    // **Proves:** AC §5 — first pass populates [`ShardedBlockCache`](dig_blockstore::cache::sharded::ShardedBlockCache); second pass must not
    // increment [`cf_blocks_stream_physical_get_count`](dig_blockstore::BlockStore::cf_blocks_stream_physical_get_count).
    //
    // **Capacity:** [`ShardedBlockCache`](dig_blockstore::cache::sharded::ShardedBlockCache) splits total capacity across
    // shards (`test_config` uses `block_cache_capacity: 10`, `cache_shards: 2` → **5 slots per shard**). A 10-block
    // chain can therefore evict within a single scan if hashes cluster in one shard; bump capacity for this test only.
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path);
    cfg.block_cache_capacity = 4096;
    let store = BlockStore::open(cfg).expect("open");
    let blocks = build_chain(12);
    for b in &blocks {
        assert!(store.put_block(b, true).expect("put"));
    }
    // `put_block` already seeds [`block_cache`](dig_blockstore::BlockStore); evict so the first stream exercises
    // `get_cf_opt` (otherwise `mid == before` and the AC §5 scenario never touches RocksDB on pass one).
    for b in &blocks {
        store.invalidate_block_cache_entry(&b.hash());
    }
    let before = store.cf_blocks_stream_physical_get_count();
    let n1: usize = store.stream_blocks_in_range(0, 11).expect("s1").count();
    let mid = store.cf_blocks_stream_physical_get_count();
    assert_eq!(n1, 12);
    assert_eq!(mid - before, 12);
    let n2: usize = store.stream_blocks_in_range(0, 11).expect("s2").count();
    let after = store.cf_blocks_stream_physical_get_count();
    assert_eq!(n2, 12);
    assert_eq!(after, mid, "second stream must be cache-only for bodies");
}

#[test]
fn test_stream_empty_when_start_gt_end() {
    // **Edge:** inverted bounds yield zero items without touching stream physical counter beyond canonical scan setup
    // (no pairs ⇒ iterator is empty).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, ZERO_HASH);
    store.put_block(&b, true).expect("put");
    let n = store.stream_blocks_in_range(5, 1).expect("build").count();
    assert_eq!(n, 0);
}

#[test]
fn test_missing_cf_blocks_row_yields_block_not_found() {
    // **Proves:** AC §6 — canonical row present, body missing ⇒ stream yields `Err(BlockNotFound(hash))`.
    let (_guard, path) = temp_blockstore_dir();
    let path_buf = path.clone();
    let victim_hash = {
        let store = BlockStore::open(test_config(path)).expect("open");
        let blocks = build_chain(8);
        for b in &blocks {
            assert!(store.put_block(b, true).expect("put"));
        }
        let h = blocks[3].hash();
        drop(store);
        h
    };
    delete_cf_blocks_row(path_buf.as_path(), &victim_hash);
    let store2 = BlockStore::open(test_config(path_buf)).expect("reopen");
    let mut stream = store2.stream_blocks_in_range(0, 7).expect("stream");
    for _ in 0..3 {
        assert!(stream.next().expect("item").is_ok());
    }
    let err = stream.next().expect("fourth").expect_err("missing body");
    match err {
        BlockStoreError::BlockNotFound(h) => assert_eq!(h, victim_hash),
        e => panic!("expected BlockNotFound, got {e:?}"),
    }
}

#[test]
fn test_stream_blocks_in_range_return_type_nameable() {
    // Compile-time guard: consumers may name [`StreamBlocksInRange`](dig_blockstore::StreamBlocksInRange) in signatures
    // ([`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md) re-export).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, ZERO_HASH);
    store.put_block(&b, true).expect("put");
    let _st: StreamBlocksInRange<'_> = store.stream_blocks_in_range(0, 0).expect("stream");
    drop(_st);
}

#[test]
#[ignore = "manual throughput comparison; BLK-006 test plan optional benchmark"]
fn test_sequential_stream_vs_random_get_block_throughput() {
    // **Intent:** Rough smoke that streaming a contiguous height band completes faster than the same blocks fetched by
    // random single-key [`get_block`](dig_blockstore::BlockStore::get_block) calls — **not** a CI gate (machine dependent).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let blocks = build_chain(80);
    for b in &blocks {
        assert!(store.put_block(b, true).expect("put"));
    }
    let t0 = Instant::now();
    let _: Vec<_> = store
        .stream_blocks_in_range(0, 79)
        .expect("stream")
        .map(|r| r.expect("ok"))
        .collect();
    let stream_ms = t0.elapsed().as_millis();
    let t1 = Instant::now();
    for b in &blocks {
        let _ = store.get_block(&b.hash()).expect("get").expect("some");
    }
    let random_ms = t1.elapsed().as_millis();
    assert!(
        stream_ms <= random_ms.saturating_mul(4),
        "stream {stream_ms}ms vs random {random_ms}ms — tune readahead / batching if this regresses badly"
    );
}
