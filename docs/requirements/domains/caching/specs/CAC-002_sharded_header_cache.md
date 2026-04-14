# CAC-002: Sharded Header Cache

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.2 |

---

## Summary

The header cache MUST use the same sharding strategy as the block cache (CAC-001) but with a separate instance and a default capacity of 2000. Headers have different access frequency patterns than full blocks and therefore require an independent cache.

---

## Specification

### Structure

The header cache reuses the `ShardedCache<V>` generic type with `V = L2BlockHeader`:

```rust
header_cache: ShardedCache<L2BlockHeader>
```

### Configuration

- Default capacity: 2000 (configurable via `header_cache_capacity`).
- Shard count: same as `cache_shards` (default 16), shared configuration with the block cache.
- Per-shard capacity: `header_cache_capacity / cache_shards`.

### Sharding Strategy

- Shard selection: `hash[0] % num_shards` (identical to block cache).
- Lock type: `parking_lot::RwLock` per shard (identical to block cache).

### Independence from Block Cache

- The header cache and block cache MUST be separate instances.
- Eviction in the header cache does NOT affect the block cache and vice versa.
- A header cache miss does NOT trigger a block cache lookup (headers are stored in `CF_HEADERS`, not extracted from blocks).

---

## Acceptance Criteria

- [ ] Default header cache capacity is 2000
- [ ] Header cache capacity is configurable
- [ ] Uses same sharding strategy as block cache (`hash[0] % num_shards`)
- [ ] Uses same `parking_lot::RwLock` per-shard locking
- [ ] Header cache and block cache are independent instances
- [ ] Evicting from one cache does not affect the other
- [ ] Header cache stores `L2BlockHeader` values (not full blocks)

---

## Implementation Notes

- The `ShardedCache<V>` type from CAC-001 should be generic enough to instantiate for both `L2Block` and `L2BlockHeader`.
- The default capacity of 2000 is higher than a typical block cache because headers are much smaller (~200 bytes vs. potentially several KB for full blocks).
- Headers are accessed more frequently than full blocks (e.g., for height lookups, ancestor traversal), so a separate cache avoids headers being evicted by block insertions.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_header_cache_default_capacity` | unit | Create with default config, verify total capacity is 2000 |
| `test_header_cache_configurable_capacity` | unit | Create with `header_cache_capacity=5000`, verify total capacity |
| `test_header_cache_shard_strategy` | unit | Verify shard selection matches block cache for same key |
| `test_header_cache_independence` | unit | Insert into header cache, verify block cache unaffected; evict from block cache, verify header cache unaffected |
| `test_header_cache_stores_headers` | unit | Insert and retrieve an `L2BlockHeader`, verify all fields match |
| `test_header_cache_lru_eviction` | unit | Fill header cache, insert beyond capacity, verify LRU entries evicted |

---

## Expected Test Files

- `tests/cac_002_tests.rs`
