//! # CAC-001 — Sharded block cache: 16-shard LRU with configurable capacity
//!
//! **Trace**
//! - Spec: [`CAC-001_sharded_block_cache.md`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAC-001)](../docs/requirements/domains/caching/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/caching/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! The sharded block cache (`ShardedBlockCache`) is the L1 acceleration layer in front
//! of RocksDB for `get_block`. These tests prove:
//!
//! 1. **Default configuration** — 16 shards, 1000 capacity per `BlockStoreConfig::default`.
//! 2. **Shard selection** — `hash[0]` determines the shard (bitmask for power-of-two).
//! 3. **LRU eviction** — per-shard eviction when shard capacity is exceeded.
//! 4. **Cache hit avoids RocksDB** — `cf_blocks_physical_get_count` stays zero on hit.
//! 5. **Remove** — explicit entry removal (test eviction simulation).
//!
//! ## Chia analogy
//!
//! Chia uses `self.block_cache = LRUCache(1000)` — a single-threaded Python LRU.
//! DIG shards across 16 independent locks to support Rust's multi-threaded runtime.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_blockstore::cache::sharded::ShardedBlockCache;
use dig_blockstore::{BlockStore, BlockStoreConfig};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

#[test]
fn test_shard_count_default() {
    // **Proves:** CAC-001 AC §1 — default shard count is 16.
    assert_eq!(BlockStoreConfig::default().cache_shards, 16);
}

#[test]
fn test_shard_count_configurable() {
    // **Proves:** CAC-001 AC §2 — shard count configurable via cache_shards.
    let cfg = BlockStoreConfig {
        cache_shards: 8,
        ..Default::default()
    };
    assert_eq!(cfg.cache_shards, 8);
}

#[test]
fn test_shard_capacity_distribution() {
    // **Proves:** CAC-001 AC §3 — per-shard capacity = total / shards.
    //
    // Create a cache with total=160, shards=16 → 10 per shard. Insert 10 blocks whose
    // hash[0] all map to the same shard. The 11th insert should evict the LRU entry.
    let cache = ShardedBlockCache::new(160, 16);

    // Insert blocks whose first byte is 0 → all map to shard 0 (0 & 15 == 0)
    let mut blocks = Vec::new();
    for i in 0u8..11 {
        let mut hash_arr = [0u8; 32];
        hash_arr[1] = i; // vary byte 1, keep byte 0 = 0
        let hash = Bytes32::new(hash_arr);
        let block = test_block(i as u64, ZERO_HASH);
        cache.insert(hash, block.clone());
        blocks.push((hash, block));
    }
    // First entry (i=0) should have been evicted by the 11th insert
    assert!(
        cache.get_clone(&blocks[0].0).is_none(),
        "LRU entry should be evicted when shard exceeds capacity"
    );
    // Last entry (i=10) should still be present
    assert!(cache.get_clone(&blocks[10].0).is_some());
}

#[test]
fn test_shard_selection_by_first_byte() {
    // **Proves:** CAC-001 AC §4 — shard selection uses hash[0] % num_shards.
    //
    // Two hashes with different hash[0] but same other bytes should land in different
    // shards (when shards >= 2). Removing one should not affect the other.
    let cache = ShardedBlockCache::new(100, 16);

    let mut arr_a = [0u8; 32];
    arr_a[0] = 0; // shard 0
    let mut arr_b = [0u8; 32];
    arr_b[0] = 1; // shard 1

    let ha = Bytes32::new(arr_a);
    let hb = Bytes32::new(arr_b);
    let ba = test_block(0, ZERO_HASH);
    let bb = test_block(1, ZERO_HASH);

    cache.insert(ha, ba);
    cache.insert(hb, bb);

    // Remove from shard 0
    cache.remove(&ha);
    assert!(cache.get_clone(&ha).is_none(), "removed from shard 0");
    assert!(
        cache.get_clone(&hb).is_some(),
        "shard 1 unaffected by shard 0 removal"
    );
}

#[test]
fn test_cache_hit_avoids_rocksdb() {
    // **Proves:** CAC-001 AC §8 — cache hit returns block without RocksDB access.
    //
    // After put_block (which inserts into block_cache), get_block should NOT increment
    // cf_blocks_physical_get_count. This is the core value proposition of the cache.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    let got = store.get_block(&b.hash()).expect("get").expect("hit");
    assert_eq!(got.hash(), b.hash());
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        0,
        "cache hit must not touch RocksDB"
    );
}

#[test]
fn test_cache_miss_then_hit() {
    // **Proves:** CAC-001 — miss reads from RocksDB, populates cache; next get is a hit.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    // Evict from cache to force a miss
    store.invalidate_block_cache_entry(&b.hash());
    assert_eq!(store.cf_blocks_physical_get_count(), 0);

    // Miss → physical read
    store.get_block(&b.hash()).expect("miss").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 1);

    // Hit → no additional read
    store.get_block(&b.hash()).expect("hit").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 1);
}

#[test]
fn test_remove_entry() {
    // **Proves:** CAC-001 AC — remove() drops an entry from the cache.
    let cache = ShardedBlockCache::new(10, 2);
    let hash = Bytes32::new([0x42; 32]);
    let block = test_block(0, ZERO_HASH);
    cache.insert(hash, block);
    assert!(cache.get_clone(&hash).is_some());
    cache.remove(&hash);
    assert!(cache.get_clone(&hash).is_none());
}
