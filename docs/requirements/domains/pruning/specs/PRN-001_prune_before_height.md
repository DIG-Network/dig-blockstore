# PRN-001: prune_before_height

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 10.1 |

---

## Summary

`prune_before_height` MUST atomically delete all blocks, headers, attestations, and canonical mappings below a given block height, evict corresponding entries from all in-memory caches, update `min_retained_height`, and return the count of pruned blocks.

---

## Specification

```rust
/// Remove all block data below the specified height.
/// Returns the number of blocks pruned.
pub fn prune_before_height(&self, height: u64) -> Result<usize>
```

### Behavior

1. Iterate `CF_CANONICAL` from `min_retained_height` (current) to `height` (exclusive), collecting height-to-hash mappings.
2. For each collected block hash:
   a. Add a delete operation for the hash key in `CF_BLOCKS`.
   b. Add a delete operation for the hash key in `CF_HEADERS`.
   c. Add a delete operation for the hash key in `CF_ATTESTED`.
   d. Add a delete operation for the height key in `CF_CANONICAL`.
3. Accumulate all deletions into a single RocksDB `WriteBatch`.
4. Execute the `WriteBatch` atomically.
5. After successful batch execution, evict each pruned hash from:
   - Block cache (all shards)
   - Header cache (all shards)
   - BlockRecord cache
   - Hash-to-height reverse cache
   - Canonical height index cache
6. Update `min_retained_height` to `height` in `CF_METADATA` under the `META_MIN_HEIGHT` key.
7. Update the in-memory `AtomicU64` for `min_retained_height`.
8. Return the count of pruned blocks.

### Atomicity

All RocksDB deletions MUST be issued in a single `WriteBatch` to ensure atomicity. If the write fails, no entries are deleted and caches remain unchanged.

### Edge Cases

- If `height <= min_retained_height`, return `Ok(0)` immediately (nothing to prune).
- If there are gaps in `CF_CANONICAL` (missing heights), those heights are skipped without error.

---

## Acceptance Criteria

- [ ] All four column families (`CF_BLOCKS`, `CF_HEADERS`, `CF_ATTESTED`, `CF_CANONICAL`) are cleaned for pruned heights
- [ ] Deletions are atomic via `WriteBatch`
- [ ] All caches are evicted for pruned entries
- [ ] `min_retained_height` is updated in both `CF_METADATA` and the `AtomicU64`
- [ ] Returns accurate count of pruned blocks
- [ ] No-op when `height <= min_retained_height`
- [ ] Blocks at `height` and above are not affected
- [ ] Gaps in canonical heights do not cause errors

---

## Implementation Notes

- `WriteBatch` provides crash-consistent atomicity: either all deletions apply or none do.
- Cache eviction happens after the `WriteBatch` succeeds. If eviction partially fails (e.g., entry already evicted), this is benign.
- For large prune ranges, consider processing in sub-batches to avoid excessive memory usage in the `WriteBatch`, though this trades atomicity for memory efficiency.
- The `AtomicU64` update should use `Ordering::Release` to ensure visibility to the compaction filter.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_prune_before_height_basic` | integration | Store 10 canonical blocks (heights 0-9), prune before 5, verify heights 0-4 removed from all CFs, heights 5-9 intact |
| `test_prune_before_height_count` | integration | Store 20 blocks, prune before 10, verify return value is 10 |
| `test_prune_before_height_caches_evicted` | integration | Store blocks, access them to populate caches, prune, verify cache misses for pruned blocks |
| `test_prune_before_height_atomicity` | integration | Verify all-or-nothing behavior: if WriteBatch contains entries from multiple CFs, all are deleted together |
| `test_prune_before_height_min_retained_updated` | integration | Prune before 5, verify META_MIN_HEIGHT is 5; prune before 8, verify META_MIN_HEIGHT is 8 |
| `test_prune_before_height_noop` | unit | Call with height <= min_retained_height, verify returns 0 and no changes |
| `test_prune_before_height_with_gaps` | integration | Store blocks at heights 0, 1, 3, 5 (gap at 2, 4), prune before 4, verify correct behavior |
| `test_prune_before_height_attestations` | integration | Store blocks with attestations, prune, verify attestations also removed |

---

## Expected Test Files

- `tests/prn_001_tests.rs`
