//! Sharded LRU cache for hot [`dig_block::L2Block`] values ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
//!
//! ## Why this exists (BLK-002)
//!
//! [`crate::store::BlockStore::get_block`](crate::store::BlockStore::get_block) MUST consult RAM before
//! [`rocksdb`](https://docs.rs/rocksdb) [`CF_BLOCKS`](crate::CF_BLOCKS) ([`BLK-002`](../../docs/requirements/domains/block_storage/specs/BLK-002.md)).
//!
//! ## Locking note vs CAC-001 prose
//!
//! The normative CAC-001 snippet uses [`parking_lot::RwLock`] **read** locks for `get`. The [`lru::LruCache`] API
//! requires `&mut self` to promote entries on access, so this implementation uses **write** locks per shard for
//! `get` + `insert`. Contention is still reduced ~`num_shards`× versus one global LRU because unrelated hashes usually
//! map to different shards ([`Self::shard_index`]).
//!
//! ## Shard selection
//!
//! [`Bytes32`](chia_protocol::Bytes32) is uniform; we use `key[0]` like CAC-001. For power-of-two shard counts we
//! apply a bitmask instead of `%` ([`CAC-001` § implementation notes](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).

use std::num::NonZeroUsize;

use chia_protocol::Bytes32;
use dig_block::L2Block;
use lru::LruCache;
use parking_lot::RwLock;

/// Production sharded LRU for block bodies ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
pub struct ShardedBlockCache {
    shards: Vec<RwLock<LruCache<Bytes32, L2Block>>>,
    num_shards: usize,
}

impl ShardedBlockCache {
    /// Build `num_shards` LRUs; each shard capacity is `max(1, total_capacity / num_shards)` ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md) § Configuration).
    ///
    /// **`num_shards`:** Clamped to ≥ 1. If not a power of two, [`Self::shard_index`] uses modulo instead of bitmask.
    pub fn new(total_capacity: usize, num_shards: usize) -> Self {
        let num_shards = num_shards.max(1);
        let per_shard = (total_capacity / num_shards).max(1);
        let nz = NonZeroUsize::new(per_shard).expect("per-shard capacity is at least 1");
        let shards = (0..num_shards)
            .map(|_| RwLock::new(LruCache::new(nz)))
            .collect();
        Self { shards, num_shards }
    }

    #[inline]
    fn shard_index(&self, key: &Bytes32) -> usize {
        let b = key.as_ref()[0] as usize;
        if self.num_shards.is_power_of_two() {
            b & (self.num_shards - 1)
        } else {
            b % self.num_shards
        }
    }

    /// Clone a cached block if present; promotes the entry inside the shard LRU.
    pub fn get_clone(&self, key: &Bytes32) -> Option<L2Block> {
        let i = self.shard_index(key);
        let mut guard = self.shards[i].write();
        guard.get(key).cloned()
    }

    /// Insert / update; may evict LRU entry in this shard only.
    pub fn insert(&self, key: Bytes32, block: L2Block) {
        let i = self.shard_index(&key);
        let mut guard = self.shards[i].write();
        guard.put(key, block);
    }

    /// Drop one entry — tests simulate eviction; future PRN / reorg hooks may call this for invalidation.
    pub fn remove(&self, key: &Bytes32) {
        let i = self.shard_index(key);
        let mut guard = self.shards[i].write();
        let _ = guard.pop(key);
    }
}
