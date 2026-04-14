# CAC-001: Sharded Block Cache

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.1 |

---

## Summary

The block cache MUST use a sharded LRU design with 16 shards by default. Each shard is an independent LRU cache protected by a `parking_lot::RwLock`. Shard selection is determined by `hash[0] % num_shards`. Sharding reduces lock contention approximately 16x under concurrent reads.

---

## Specification

### Structure

```rust
pub struct ShardedCache<V> {
    shards: Vec<RwLock<LruCache<Bytes32, V>>>,
    num_shards: usize,
}
```

### Configuration

- `cache_shards`: Number of shards (default: 16). MUST be a power of 2 for optimal distribution.
- `block_cache_capacity`: Total capacity across all shards. Each shard's capacity = `block_cache_capacity / cache_shards`.

### Shard Selection

```rust
fn shard_index(&self, key: &Bytes32) -> usize {
    key.as_ref()[0] as usize % self.num_shards
}
```

The first byte of the block hash is used as the shard selector. Since block hashes are cryptographic hashes, the first byte has uniform distribution across 0-255, providing good shard balance.

### Operations

- **get(key)**: Acquire read lock on the target shard, look up the key in the shard's LRU. Promote to most-recently-used on hit.
- **insert(key, value)**: Acquire write lock on the target shard, insert into the shard's LRU. Evict least-recently-used entry if shard is at capacity.
- **remove(key)**: Acquire write lock on the target shard, remove the key from the shard's LRU.

### Lock Strategy

- Each shard has its own `parking_lot::RwLock`.
- Read operations acquire a read lock (multiple concurrent readers per shard).
- Write operations acquire a write lock (exclusive access per shard).
- Operations on different shards are fully independent (no cross-shard locking).

---

## Acceptance Criteria

- [ ] Default shard count is 16
- [ ] Shard count is configurable via `cache_shards`
- [ ] Each shard capacity equals `block_cache_capacity / cache_shards`
- [ ] Shard selection uses `hash[0] % num_shards`
- [ ] Each shard is protected by an independent `parking_lot::RwLock`
- [ ] LRU eviction operates independently per shard
- [ ] Concurrent reads on different shards do not contend
- [ ] Cache hit returns the stored block without RocksDB access

---

## Implementation Notes

- `parking_lot::RwLock` is chosen over `std::sync::RwLock` for its smaller memory footprint and lack of poisoning behavior.
- The `lru` crate's `LruCache` provides O(1) get, put, and eviction.
- For `num_shards` that is a power of 2, the modulo can be replaced with a bitwise AND: `hash[0] as usize & (num_shards - 1)`.
- The sharded design means total effective capacity may be slightly less than `block_cache_capacity` if entries are unevenly distributed, but with cryptographic hashes this is negligible.
- Consider implementing `ShardedCache` as a generic type reusable for both block and header caches.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_shard_count_default` | unit | Create cache with default config, verify 16 shards |
| `test_shard_count_configurable` | unit | Create cache with `cache_shards=8`, verify 8 shards |
| `test_shard_capacity_distribution` | unit | Create cache with capacity 160 and 16 shards, verify each shard capacity is 10 |
| `test_shard_selection` | unit | Insert keys with known first bytes, verify they land in expected shards |
| `test_lru_eviction_per_shard` | unit | Fill one shard to capacity, insert one more, verify LRU entry evicted |
| `test_cache_hit` | unit | Insert a block, get it back, verify match without RocksDB access |
| `test_cache_miss` | unit | Query a key not in cache, verify None returned |
| `test_concurrent_reads` | unit | Spawn 16 threads reading different shards simultaneously, verify no deadlock and correct values |
| `test_concurrent_read_write` | unit | Spawn readers and writers on overlapping shards, verify correctness under contention |
| `test_remove` | unit | Insert a key, remove it, verify subsequent get returns None |

---

## Expected Test Files

- `tests/test_cac_001_sharded_block_cache.rs`
