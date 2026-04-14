# CAC-005: Hash-to-Height Reverse Cache

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.5 |

---

## Summary

The hash-to-height reverse lookup cache SHOULD map `Bytes32` to `u64` for recently accessed blocks. It is used by `find_common_ancestor` and `set_canonical` to avoid deserializing headers solely for height lookups.

---

## Specification

### Structure

```rust
hash_to_height: ShardedCache<u64>
```

Uses the same `ShardedCache` infrastructure as the block and header caches for consistent sharding and lock behavior.

### Behavior

- **Lookup**: `get_height(hash)` checks the cache. On hit, returns the height in O(1) time. On miss, falls through to `CF_HEADERS` (deserialize header, extract height).
- **Population**: The cache MUST be updated when:
  - A block is stored via `put_block` (height extracted from header).
  - A header is read from `CF_HEADERS` on cache miss (opportunistic population).
  - A block is read from `CF_BLOCKS` on cache miss.
- **Eviction**: Entries are evicted by LRU policy when the cache is at capacity, and explicitly evicted during pruning.

### Primary Consumers

| Consumer | Usage |
|----------|-------|
| `find_common_ancestor` | Needs heights of blocks along fork chains to walk backwards; avoids O(n) header deserializations |
| `set_canonical` | Needs the height of the block being set as canonical for the `CF_CANONICAL` mapping |
| `prune_before_height` | Used by non-canonical pruning (PRN-005) to determine heights of non-canonical blocks |

### Fallback

On cache miss, the height MUST be obtained by reading the header from `CF_HEADERS`, deserializing it with bincode, and extracting the `height` field. The result SHOULD then be inserted into the cache.

---

## Acceptance Criteria

- [ ] Maps `Bytes32` (block hash) to `u64` (block height)
- [ ] Cache hit returns height without header deserialization
- [ ] Cache populated on `put_block`, header read, and block read
- [ ] Used by `find_common_ancestor` to avoid repeated header lookups
- [ ] Used by `set_canonical` for height-to-hash mapping
- [ ] LRU eviction when at capacity
- [ ] Explicit eviction during pruning
- [ ] Cache miss falls through to `CF_HEADERS` deserialization

---

## Implementation Notes

- The value stored is a simple `u64` (8 bytes), so this cache has minimal memory overhead per entry.
- Consider a larger default capacity than the block cache since entries are tiny.
- The sharded design (from `ShardedCache`) ensures the same lock-contention benefits as the block cache.
- This cache is the inverse of the canonical height index (CAC-004). Together they provide bidirectional O(1)/O(log n) lookups between hashes and heights.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_hash_to_height_cache_hit` | unit | Insert hash-height mapping, lookup, verify height returned |
| `test_hash_to_height_cache_miss` | unit | Lookup hash not in cache, verify fallthrough to CF_HEADERS |
| `test_hash_to_height_populated_on_put` | unit | Call put_block, verify hash-to-height entry exists in cache |
| `test_hash_to_height_populated_on_header_read` | unit | Read a header (cache miss), verify hash-to-height populated |
| `test_hash_to_height_lru_eviction` | unit | Fill cache to capacity, insert new entry, verify LRU entry evicted |
| `test_hash_to_height_pruning_evicts` | integration | Prune blocks, verify hash-to-height entries evicted for pruned hashes |
| `test_hash_to_height_find_common_ancestor` | integration | Set up fork, call find_common_ancestor, verify cache hits reduce header reads |
| `test_hash_to_height_set_canonical_uses_cache` | integration | Populate cache, call set_canonical, verify no header deserialization needed |

---

## Expected Test Files

- `tests/cac_005_tests.rs`
