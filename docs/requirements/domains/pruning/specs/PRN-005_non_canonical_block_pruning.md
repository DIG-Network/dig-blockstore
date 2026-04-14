# PRN-005: Non-Canonical Block Pruning

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 10.1 |

---

## Summary

Pruning MUST remove non-canonical blocks at pruned heights in addition to canonical blocks. When iterating `CF_BLOCKS` during pruning, any block whose height (determined from its header) is below the target pruning height MUST be removed regardless of whether it is part of the canonical chain.

---

## Specification

### Behavior

During `prune_before_height(height)`, after processing canonical entries via `CF_CANONICAL`, the pruning logic MUST also scan for non-canonical blocks:

1. Iterate `CF_BLOCKS` (or `CF_HEADERS` for efficiency).
2. For each entry, determine the block's height by looking up the header in `CF_HEADERS` and extracting the height field.
3. If `block_height < height`, add delete operations for:
   - The block hash key in `CF_BLOCKS`
   - The block hash key in `CF_HEADERS`
   - The block hash key in `CF_ATTESTED`
4. Include these deletions in the same `WriteBatch` used for canonical pruning.
5. Evict the pruned non-canonical entries from all in-memory caches.

### Rationale

Non-canonical blocks (e.g., from forks or reorganizations) are stored in `CF_BLOCKS` and `CF_HEADERS` but do not have entries in `CF_CANONICAL`. Without explicit non-canonical pruning, these orphaned blocks would accumulate indefinitely, consuming disk space.

### Interaction with Compaction Filter

- The compaction filter (PRN-003) serves as a secondary mechanism that also catches non-canonical blocks during background compaction.
- Explicit non-canonical pruning in `prune_before_height` provides immediate cleanup without waiting for compaction.

---

## Acceptance Criteria

- [ ] Non-canonical blocks below the pruning height are deleted from `CF_BLOCKS`
- [ ] Non-canonical headers below the pruning height are deleted from `CF_HEADERS`
- [ ] Non-canonical attestations below the pruning height are deleted from `CF_ATTESTED`
- [ ] Non-canonical deletions are included in the same `WriteBatch` as canonical deletions
- [ ] Caches are evicted for non-canonical pruned entries
- [ ] Non-canonical blocks at or above the pruning height are retained
- [ ] Canonical blocks are unaffected by the non-canonical scan (no double-deletion issues)

---

## Implementation Notes

- Iterating all of `CF_BLOCKS` or `CF_HEADERS` to find non-canonical blocks is an O(n) scan over the entire column family. This may be expensive for large stores.
- Optimization: maintain a secondary index of non-canonical blocks by height, or scan `CF_HEADERS` (smaller values) instead of `CF_BLOCKS` to minimize I/O.
- The canonical pruning pass (PRN-001) already collects hashes from `CF_CANONICAL`. The non-canonical pass should skip hashes already handled by the canonical pass to avoid redundant work.
- Consider running the non-canonical scan in a background task if it is too expensive to run synchronously.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_non_canonical_pruned` | integration | Store canonical block at height 5, store non-canonical block at height 5 (different hash), prune before 6, verify both removed |
| `test_non_canonical_above_height_retained` | integration | Store non-canonical block at height 10, prune before 5, verify non-canonical block retained |
| `test_non_canonical_mixed_heights` | integration | Store non-canonical blocks at heights 3, 7, 12; prune before 10; verify heights 3, 7 removed and height 12 retained |
| `test_non_canonical_no_double_delete` | integration | Store canonical block at height 5, prune before 6, verify no error from attempting to delete the same hash twice |
| `test_non_canonical_caches_evicted` | integration | Access non-canonical blocks to populate caches, prune, verify cache misses |
| `test_non_canonical_attestations_pruned` | integration | Store attestation for non-canonical block at pruned height, verify attestation also removed |

---

## Expected Test Files

- `tests/prn_005_tests.rs`
