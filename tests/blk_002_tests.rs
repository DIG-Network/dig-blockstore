//! # BLK-002 — `get_block`: block cache first, then RocksDB + zstd + bincode
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-002.md`](../docs/requirements/domains/block_storage/specs/BLK-002.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-002)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-002-get-block-by-hash-get_block)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//! - Cache layout: [`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Stored block round-trips | [`test_put_get_round_trip_hash`] |
//! | §2 | Cache hit ⇒ no `CF_BLOCKS` `get_cf` | [`test_cache_hit_does_not_increment_physical_get_count`] |
//! | §3 | Miss ⇒ read-through + repopulate LRU | [`test_cache_miss_read_through_then_hit`] |
//! | §4–§5 | Dictionary path + plain fallback ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)) | [`test_dictionary_compressed_round_trip_after_cache_evict`], [`test_plain_zstd_on_disk_readable_with_dict_override`] |
//! | §6 | Unknown hash ⇒ `None` | [`test_unknown_hash_returns_none_and_counts_physical_read`] |
//!
//! **Instrumentation:** [`BlockStore::cf_blocks_physical_get_count`](dig_blockstore::BlockStore::cf_blocks_physical_get_count)
//! counts post-cache-miss [`CF_BLOCKS`] reads inside [`get_block`](dig_blockstore::BlockStore::get_block) (see `store.rs`).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::L2Block;
use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// Train a **valid** zstd dictionary — needs enough total sample bytes (`zstd::dict::from_samples`); see [`SER-004`](ser_004_tests.rs).
fn train_zstd_dict_from_blocks(blocks: &[L2Block]) -> Vec<u8> {
    let samples: Vec<Vec<u8>> = blocks
        .iter()
        .map(|b| bincode::serialize(b).expect("bincode L2Block"))
        .collect();
    let refs: Vec<&[u8]> = samples.iter().map(Vec::as_slice).collect();
    zstd::dict::from_samples(&refs, 64 * 1024).expect("dictionary training")
}

#[test]
fn test_put_get_round_trip_hash() {
    // **Proves:** AC §1 — a block inserted via [`BlockStore::put_block`] must decode to the same [`L2Block::hash`].
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(11, ZERO_HASH);
    assert!(store.put_block(&b, false).expect("put"));
    let got = store.get_block(&b.hash()).expect("get").expect("some");
    assert_eq!(got.hash(), b.hash());
}

#[test]
fn test_cache_hit_does_not_increment_physical_get_count() {
    // **Proves:** AC §2 — after [`put_block`] seeds [`BlockStore::block_cache`], repeated [`get_block`] calls must not
    // increment [`cf_blocks_physical_get_count`](dig_blockstore::BlockStore::cf_blocks_physical_get_count).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(3, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    store.get_block(&b.hash()).expect("get1").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    store.get_block(&b.hash()).expect("get2").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
}

#[test]
fn test_cache_miss_read_through_then_hit() {
    // **Proves:** AC §3 — evict LRU entry with [`invalidate_block_cache_entry`](dig_blockstore::BlockStore::invalidate_block_cache_entry),
    // next [`get_block`] performs one physical read, repopulates cache; following get is free (count stable).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(4, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_block_cache_entry(&h);
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    store.get_block(&h).expect("miss").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 1);
    store.get_block(&h).expect("hit").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 1);
}

#[test]
fn test_unknown_hash_returns_none_and_counts_physical_read() {
    // **Proves:** AC §6 — probes that miss in LRU **and** RocksDB return `Ok(None)` but still account as a physical read
    // (observability contract for “we touched `CF_BLOCKS`”).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let ghost = Bytes32::new([7u8; 32]);
    assert!(store.get_block(&ghost).expect("get").is_none());
    assert_eq!(store.cf_blocks_physical_get_count(), 1);
}

#[test]
fn test_dictionary_compressed_round_trip_after_cache_evict() {
    // **Proves:** AC §4 — with [`BlockStoreConfig::use_compression_dict`] + valid [`zstd_dictionary_override`], payloads are
    // dictionary-compressed; [`deserialize_block`](dig_blockstore::BlockStore::deserialize_block) uses dict decompress first.
    let (_guard, path) = temp_blockstore_dir();
    let dict = train_zstd_dict_from_blocks(&build_chain(32));
    let mut cfg = test_config(path);
    cfg.use_compression_dict = true;
    cfg.zstd_dictionary_override = Some(dict);
    let store = BlockStore::open(cfg).expect("open");
    let b = test_block(5, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_block_cache_entry(&h);
    let got = store.get_block(&h).expect("get").expect("some");
    assert_eq!(got.hash(), h);
}

#[test]
fn test_plain_zstd_on_disk_readable_with_dict_override() {
    // **Proves:** AC §5 — blocks written **before** a dictionary was in use (plain zstd frames) must still decode when
    // the store is later opened with [`use_compression_dict`] + a trained dictionary ([`SER-005`] fallback in `decompress_block_payload`).
    let (_guard, path) = temp_blockstore_dir();
    let b = test_block(8, ZERO_HASH);
    {
        let mut cfg = test_config(path.clone());
        cfg.use_compression_dict = false;
        let store = BlockStore::open(cfg).expect("open plain");
        store.put_block(&b, false).expect("put");
    }
    let dict = train_zstd_dict_from_blocks(&build_chain(32));
    let mut cfg2 = test_config(path);
    cfg2.use_compression_dict = true;
    cfg2.zstd_dictionary_override = Some(dict);
    let store2 = BlockStore::open(cfg2).expect("reopen with dict");
    assert_eq!(store2.cf_blocks_physical_get_count(), 0);
    let got = store2.get_block(&b.hash()).expect("get").expect("some");
    assert_eq!(got.hash(), b.hash());
    assert_eq!(store2.cf_blocks_physical_get_count(), 1);
    let got2 = store2.get_block(&b.hash()).expect("get2").expect("some");
    assert_eq!(got2.hash(), b.hash());
    assert_eq!(store2.cf_blocks_physical_get_count(), 1);
}
