//! # CAC-002 — Sharded header cache: independent LRU for L2BlockHeader values
//!
//! **Trace**
//! - Spec: [`CAC-002_sharded_header_cache.md`](../docs/requirements/domains/caching/specs/CAC-002_sharded_header_cache.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAC-002)](../docs/requirements/domains/caching/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/caching/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! The header cache (`ShardedHeaderCache`) is separate from the block cache and uses
//! the same sharding strategy. These tests prove:
//!
//! 1. **Default capacity** — 2000 per `BlockStoreConfig::default`.
//! 2. **Independence** — evicting from the block cache does not affect the header cache.
//! 3. **Cache hit avoids RocksDB** — `cf_headers_physical_get_count` stays zero on hit.
//! 4. **Stores L2BlockHeader** — round-trip through the cache preserves all header fields.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_blockstore::cache::sharded::ShardedHeaderCache;
use dig_blockstore::{BlockStore, BlockStoreConfig};

use common::{temp_blockstore_dir, test_block, test_config, test_header};

#[test]
fn test_header_cache_default_capacity() {
    // **Proves:** CAC-002 AC §1 — default capacity is 2000.
    assert_eq!(BlockStoreConfig::default().header_cache_capacity, 2000);
}

#[test]
fn test_header_cache_configurable_capacity() {
    // **Proves:** CAC-002 AC §2 — capacity is configurable.
    let cfg = BlockStoreConfig {
        header_cache_capacity: 5000,
        ..Default::default()
    };
    assert_eq!(cfg.header_cache_capacity, 5000);
}

#[test]
fn test_header_cache_stores_headers() {
    // **Proves:** CAC-002 AC §7 — header cache stores L2BlockHeader values with all fields intact.
    let cache = ShardedHeaderCache::new(100, 4);
    let header = test_header(42, Bytes32::new([7u8; 32]));
    let hash = header.hash();
    cache.insert(hash, header.clone());
    let got = cache.get_clone(&hash).expect("should be cached");
    assert_eq!(
        got, header,
        "all header fields must survive cache round-trip"
    );
}

#[test]
fn test_header_cache_independence_from_block_cache() {
    // **Proves:** CAC-002 AC §5-§6 — header and block caches are independent.
    //
    // Evicting a block from the block cache must NOT affect the header cache.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    // Both caches should be warm from init_genesis
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_header(&b.hash()).expect("header hit");
    assert_eq!(
        store.cf_headers_physical_get_count(),
        0,
        "header served from cache"
    );

    // Evict from block cache only
    store.invalidate_block_cache_entry(&b.hash());

    // Header cache should still serve the header
    store.get_header(&b.hash()).expect("header still cached");
    assert_eq!(
        store.cf_headers_physical_get_count(),
        0,
        "header cache unaffected by block cache eviction"
    );
}

#[test]
fn test_header_cache_hit_avoids_rocksdb() {
    // **Proves:** CAC-002 — cache hit returns header without CF_HEADERS access.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    assert_eq!(store.cf_headers_physical_get_count(), 0);
    let h = store.get_header(&b.hash()).expect("get").expect("hit");
    assert_eq!(h, b.header);
    assert_eq!(store.cf_headers_physical_get_count(), 0);
}

#[test]
fn test_header_cache_miss_then_hit() {
    // **Proves:** CAC-002 — miss reads from RocksDB, then next get is a hit.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    store.invalidate_header_cache_entry(&b.hash());
    store.get_header(&b.hash()).expect("miss").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);

    store.get_header(&b.hash()).expect("hit").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);
}

#[test]
fn test_header_cache_lru_eviction() {
    // **Proves:** CAC-002 AC §6 — LRU eviction when shard exceeds capacity.
    let cache = ShardedHeaderCache::new(4, 1); // 1 shard, capacity 4
    let mut headers = Vec::new();
    for i in 0u8..5 {
        let h = test_header(i as u64, ZERO_HASH);
        let hash = h.hash();
        cache.insert(hash, h.clone());
        headers.push((hash, h));
    }
    // First entry should have been evicted
    assert!(
        cache.get_clone(&headers[0].0).is_none(),
        "LRU entry evicted"
    );
    assert!(
        cache.get_clone(&headers[4].0).is_some(),
        "most recent still present"
    );
}
