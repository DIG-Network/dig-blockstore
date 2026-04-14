# CAC-004: Canonical Height Index Cache

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.4 |

---

## Summary

The canonical height index cache SHOULD maintain an in-memory `BTreeMap<u64, Bytes32>` for recent canonical height-to-hash mappings. It provides O(log n) lookups for hot heights without requiring mmap access to `CF_CANONICAL`. The cache is populated from mmap reads and evicted on rollback.

---

## Specification

### Structure

```rust
canonical_index: RwLock<BTreeMap<u64, Bytes32>>
```

### Behavior

- **Lookup**: `get_canonical_hash(height)` checks the `BTreeMap` first. On hit, returns the hash in O(log n) time without accessing RocksDB or mmap. On miss, falls through to `CF_CANONICAL`.
- **Population**: When a canonical hash is read from `CF_CANONICAL` (via mmap or RocksDB), the result SHOULD be inserted into the `BTreeMap`.
- **Insert**: When `set_canonical(hash, height)` is called, the mapping MUST be inserted into the `BTreeMap`.
- **Eviction on Rollback**: When a rollback removes canonical status from a block at height H, the entry at height H MUST be removed from the `BTreeMap`. If the rollback replaces height H with a different hash, the entry MUST be updated.

### Why BTreeMap

- `BTreeMap` is used instead of `HashMap` because it supports efficient range queries (e.g., "get all canonical hashes between heights 100 and 200") and ordered iteration.
- O(log n) lookup is acceptable for an in-memory index where n is bounded by the number of cached heights.

### Bounded Size

- The cache SHOULD be bounded to prevent unbounded memory growth.
- Consider retaining only the most recent N heights (e.g., the last 10,000 canonical mappings).
- Older entries can be evicted when the cache exceeds its bound.

---

## Acceptance Criteria

- [ ] Maintains an in-memory `BTreeMap<u64, Bytes32>` for canonical mappings
- [ ] Provides O(log n) lookups for cached heights
- [ ] Cache hit avoids mmap/RocksDB access
- [ ] Populated from `CF_CANONICAL` reads and `set_canonical` calls
- [ ] Entries evicted on rollback
- [ ] Updated entries reflect the new canonical hash after rollback-and-replace
- [ ] Cache does not grow unbounded

---

## Implementation Notes

- The `BTreeMap` should be protected by a single `RwLock` since canonical height lookups are typically sequential or narrow-range, making sharding unnecessary.
- For bounded size, consider wrapping with a max-size check on insert: if `len > max_cached_heights`, remove the entry with the smallest height.
- The canonical index cache complements the hash-to-height reverse cache (CAC-005): this cache maps height-to-hash, while CAC-005 maps hash-to-height.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_canonical_index_cache_hit` | unit | Insert mapping, lookup by height, verify hash returned without RocksDB access |
| `test_canonical_index_cache_miss` | unit | Lookup height not in cache, verify fallthrough to CF_CANONICAL |
| `test_canonical_index_set_canonical` | unit | Call set_canonical, verify mapping added to BTreeMap |
| `test_canonical_index_rollback_eviction` | unit | Insert mapping, simulate rollback at that height, verify entry removed |
| `test_canonical_index_rollback_replacement` | unit | Insert mapping at height H, rollback and replace with new hash, verify updated |
| `test_canonical_index_bounded_size` | unit | Insert more than max entries, verify oldest entries evicted |
| `test_canonical_index_range_query` | unit | Insert mappings for heights 100-200, query range, verify ordered results |

---

## Expected Test Files

- `tests/test_cac_004_canonical_height_index_cache.rs`
