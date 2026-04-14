# CAC-006: Cache Warming on Startup

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 11.6 |

---

## Summary

When `warm_cache_on_open` is `true`, `BlockStore::open()` MUST preload the most recent N blocks and headers into their respective caches by reading the canonical chain backwards from the tip. Cache warming SHOULD complete before `open()` returns.

---

## Specification

### Trigger

Cache warming is triggered during `BlockStore::open()` when the configuration flag `warm_cache_on_open` is `true`. When `false`, the caches start empty and are populated on demand.

### Algorithm

```
1. Determine the chain tip height from CF_CANONICAL (reverse iterator, highest key).
2. Calculate start_height = max(tip_height - block_cache_capacity + 1, min_retained_height).
3. Iterate CF_CANONICAL from tip_height backwards to start_height.
4. For each height:
   a. Read the canonical hash from CF_CANONICAL.
   b. Read the block from CF_BLOCKS, decompress, deserialize, insert into block cache.
   c. Read the header from CF_HEADERS, deserialize, insert into header cache.
   d. Derive BlockRecord via from_header(), insert into record cache.
   e. Insert hash-to-height mapping into hash-to-height cache.
   f. Insert height-to-hash mapping into canonical index cache.
5. Return from open() with warm caches.
```

### Quantity

- N = `block_cache_capacity` (the total block cache capacity).
- If the chain has fewer than N blocks, all available blocks are loaded.
- Blocks below `min_retained_height` are NOT loaded (they have been pruned).

### Completion Guarantee

- Cache warming SHOULD complete before `BlockStore::open()` returns.
- The caller should expect that `open()` takes longer when `warm_cache_on_open` is `true`, proportional to N and disk I/O speed.

### Error Handling

- Errors during cache warming (e.g., corrupted block, missing entry) SHOULD be logged but MUST NOT prevent `open()` from succeeding. The cache will have partial warmth; subsequent reads will fill the gaps on demand.

---

## Acceptance Criteria

- [ ] Cache warming occurs when `warm_cache_on_open` is `true`
- [ ] Cache warming does NOT occur when `warm_cache_on_open` is `false`
- [ ] Reads canonical chain backwards from tip
- [ ] Loads up to `block_cache_capacity` blocks and headers
- [ ] Block cache, header cache, record cache, hash-to-height cache, and canonical index cache are all populated
- [ ] Does not attempt to load pruned blocks (below `min_retained_height`)
- [ ] Completes before `open()` returns
- [ ] Errors during warming do not prevent `open()` from succeeding
- [ ] Empty store (no blocks) completes warming without error

---

## Implementation Notes

- Cache warming is I/O-bound. Consider using RocksDB readahead to optimize sequential backward reads.
- For very large `block_cache_capacity`, warming could take significant time. Consider logging progress (e.g., "warming cache: loaded 500/1000 blocks").
- The backward iteration over `CF_CANONICAL` is efficient because keys are big-endian u64, so reverse iteration naturally yields descending heights.
- Cache warming populates all five cache layers in a single pass, which is more efficient than letting each cache warm independently on demand.
- Consider exposing a `warming_progress` callback or metric for monitoring.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_warming_enabled` | integration | Store 100 blocks, open with `warm_cache_on_open=true`, verify block cache populated for recent blocks |
| `test_warming_disabled` | integration | Store 100 blocks, open with `warm_cache_on_open=false`, verify caches empty |
| `test_warming_all_caches` | integration | Open with warming, verify block cache, header cache, record cache, hash-to-height cache, and canonical index cache all populated |
| `test_warming_respects_capacity` | integration | Store 200 blocks, set `block_cache_capacity=50`, open with warming, verify only 50 most recent blocks cached |
| `test_warming_fewer_blocks_than_capacity` | integration | Store 10 blocks, set `block_cache_capacity=100`, open with warming, verify all 10 blocks cached |
| `test_warming_respects_min_retained_height` | integration | Store 100 blocks, prune before 50, reopen with warming, verify only blocks 50-99 cached |
| `test_warming_empty_store` | integration | Open empty store with warming enabled, verify no error and caches empty |
| `test_warming_completes_before_open_returns` | integration | Open with warming, immediately query recent blocks, verify cache hits (no RocksDB reads) |
| `test_warming_corrupted_block_nonfatal` | integration | Store blocks, corrupt one block's bytes in CF_BLOCKS, open with warming, verify open succeeds and other blocks cached |

---

## Expected Test Files

- `tests/cac_006_tests.rs`
