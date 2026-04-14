# CAC-003: BlockRecord Cache

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.3 |

---

## Summary

The `BlockRecord` cache MUST be in-memory only (never persisted to disk). On cache miss, `BlockRecord` values are derived from headers via `from_header()`. The cache MUST be updated on `put_block`, `update_status`, and `set_canonical` operations.

---

## Specification

### Structure

```rust
record_cache: ShardedCache<BlockRecord>
```

### Cache Miss Behavior

When `get_record(hash)` encounters a cache miss:

1. Look up the header in `CF_HEADERS` (or the header cache).
2. Call `BlockRecord::from_header(&header)` to derive a `BlockRecord`.
3. Insert the derived record into the record cache.
4. Return the record.

### Cache Update Triggers

The record cache MUST be updated in the following operations:

| Operation | Cache Action |
|-----------|-------------|
| `put_block(block, canonical)` | Insert new `BlockRecord` derived from block header |
| `update_status(hash, status)` | Update the `status` field of the cached record |
| `set_canonical(hash, height)` | Update the `in_canonical_chain` flag to `true` |
| Rollback (unset canonical) | Update the `in_canonical_chain` flag to `false` |
| Pruning | Evict the record from the cache |

### No Persistence

- `BlockRecord` values are NEVER written to any RocksDB column family.
- They exist only in the in-memory cache.
- On restart, the cache starts empty and is populated on demand via `from_header()`.

---

## Acceptance Criteria

- [ ] `BlockRecord` values are never written to disk
- [ ] Cache miss triggers `from_header()` derivation from the header
- [ ] `put_block` inserts a new record into the cache
- [ ] `update_status` modifies the cached record's status field
- [ ] `set_canonical` updates the `in_canonical_chain` flag
- [ ] Pruning evicts records from the cache
- [ ] After restart, cache is empty (not restored from disk)
- [ ] Derived `BlockRecord` matches the header fields

---

## Implementation Notes

- `BlockRecord::from_header()` extracts: hash, height, parent_hash, timestamp, and sets default values for status and canonical flag.
- Since `BlockRecord` is lightweight (no transaction data), the cache can hold many entries without significant memory pressure.
- The `update_status` operation must acquire a write lock on the relevant shard to modify the cached value in-place.
- Consider using `ShardedCache<RwLock<BlockRecord>>` or mutating through the LRU cache's mutable access API to allow in-place updates without full replacement.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_record_cache_miss_derives_from_header` | unit | Store a header in CF_HEADERS, call get_record (cache miss), verify record derived from header |
| `test_record_cache_hit` | unit | Put a block, then get_record for same hash, verify cache hit (no CF_HEADERS read) |
| `test_record_cache_put_block_inserts` | unit | Call put_block, verify record cache contains a record for the block hash |
| `test_record_cache_update_status` | unit | Put a block, update_status, verify cached record has new status |
| `test_record_cache_set_canonical` | unit | Put a block, set_canonical, verify cached record has in_canonical_chain=true |
| `test_record_cache_rollback` | unit | Set canonical, then unset (rollback), verify in_canonical_chain=false |
| `test_record_cache_not_persisted` | integration | Put blocks, close store, reopen, verify record cache is empty |
| `test_record_cache_pruning_evicts` | integration | Put blocks, populate record cache, prune, verify records evicted |
| `test_record_from_header_accuracy` | unit | Create header with known fields, derive record, verify all fields match |

---

## Expected Test Files

- `tests/test_cac_003_block_record_cache.rs`
