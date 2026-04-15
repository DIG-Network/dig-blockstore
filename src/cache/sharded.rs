//! Sharded LRU caches for hot dig-block values ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
//!
//! ## Types
//!
//! - [`ShardedBlockCache`] — [`dig_block::L2Block`] bodies ([`BLK-002`](../../docs/requirements/domains/block_storage/specs/BLK-002.md)).
//! - [`ShardedHeaderCache`] — [`dig_block::L2BlockHeader`] rows ([`BLK-003`](../../docs/requirements/domains/block_storage/specs/BLK-003.md), [`CAC-002`](../../docs/requirements/domains/caching/specs/CAC-002_sharded_header_cache.md) precursor).
//!
//! Both are aliases over [`ShardedLruCache`], which centralizes shard math and eviction policy.
//!
//! ## Locking note vs CAC-001 prose
//!
//! The normative CAC-001 snippet uses [`parking_lot::RwLock`] **read** locks for `get`. The [`lru::LruCache`] API
//! requires `&mut self` to promote entries on access, so this implementation uses **write** locks per shard for
//! `get_clone` + `insert`. Contention is still reduced ~`num_shards`× versus one global LRU.
//!
//! ## Shard selection
//!
//! [`Bytes32`](chia_protocol::Bytes32) is uniform; we use `key[0]` like CAC-001. For power-of-two shard counts we
//! apply a bitmask instead of `%`.

use std::num::NonZeroUsize;

use chia_protocol::Bytes32;
use dig_block::{L2Block, L2BlockHeader};
use lru::LruCache;
use parking_lot::RwLock;

/// Generic sharded LRU keyed by block hash ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
pub struct ShardedLruCache<V: Clone> {
    shards: Vec<RwLock<LruCache<Bytes32, V>>>,
    num_shards: usize,
}

impl<V: Clone> ShardedLruCache<V> {
    /// Build `num_shards` LRUs; each shard capacity is `max(1, total_capacity / num_shards)`.
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

    /// Clone a cached value if present; promotes the entry inside the shard LRU.
    pub fn get_clone(&self, key: &Bytes32) -> Option<V> {
        let i = self.shard_index(key);
        let mut guard = self.shards[i].write();
        guard.get(key).cloned()
    }

    /// Insert / update; may evict LRU entry in this shard only.
    pub fn insert(&self, key: Bytes32, value: V) {
        let i = self.shard_index(&key);
        let mut guard = self.shards[i].write();
        guard.put(key, value);
    }

    /// Membership probe **without** LRU promotion ([`LruCache::peek`](lru::LruCache::peek)).
    ///
    /// **Rationale:** [`Self::get_clone`] must take a write lock because [`LruCache::get`] mutates recency; existence-only
    /// checks for [`BLK-011`](../../docs/requirements/domains/block_storage/specs/BLK-011.md) should not reorder hot entries
    /// when answering “is this hash cached?”.
    #[inline]
    #[must_use]
    pub fn contains(&self, key: &Bytes32) -> bool {
        let i = self.shard_index(key);
        let guard = self.shards[i].read();
        guard.peek(key).is_some()
    }

    /// Drop one entry — tests simulate eviction; future invalidation / PRN hooks may reuse this.
    pub fn remove(&self, key: &Bytes32) {
        let i = self.shard_index(key);
        let mut guard = self.shards[i].write();
        let _ = guard.pop(key);
    }
}

/// Sharded LRU for block **bodies** ([`BLK-002`](../../docs/requirements/domains/block_storage/specs/BLK-002.md)).
pub type ShardedBlockCache = ShardedLruCache<L2Block>;

/// Sharded LRU for **headers** ([`BLK-003`](../../docs/requirements/domains/block_storage/specs/BLK-003.md)).
pub type ShardedHeaderCache = ShardedLruCache<L2BlockHeader>;
