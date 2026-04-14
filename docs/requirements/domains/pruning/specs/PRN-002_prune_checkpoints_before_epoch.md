# PRN-002: prune_checkpoints_before_epoch

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 10.2 |

---

## Summary

`prune_checkpoints_before_epoch` MUST iterate `CF_CHECKPOINTS` from epoch 0 up to (but not including) the specified epoch, delete each entry, and return the count of pruned checkpoints.

---

## Specification

```rust
/// Remove all checkpoints with epochs below the specified epoch.
/// Returns the number of checkpoints pruned.
pub fn prune_checkpoints_before_epoch(&self, epoch: u64) -> Result<usize>
```

### Behavior

1. Encode epoch 0 as a big-endian 8-byte key (start of iteration).
2. Create a RocksDB forward iterator on `CF_CHECKPOINTS`, seeking to the start.
3. For each entry:
   a. Decode the key as a big-endian u64 epoch.
   b. If `decoded_epoch >= epoch`, stop iteration.
   c. Delete the entry from `CF_CHECKPOINTS`.
   d. Increment the prune count.
4. Return the total count of deleted entries.

### Edge Cases

- If `epoch` is 0, return `Ok(0)` immediately (nothing to prune).
- If no checkpoints exist below `epoch`, return `Ok(0)`.
- Gaps in epoch numbers are expected and handled naturally by the iterator.

---

## Acceptance Criteria

- [ ] Iterates `CF_CHECKPOINTS` from epoch 0 up to (exclusive) the target epoch
- [ ] Deletes each checkpoint entry found in the range
- [ ] Returns accurate count of pruned checkpoints
- [ ] Checkpoints at the target epoch and above are not affected
- [ ] Returns 0 when no checkpoints exist below the target
- [ ] Returns 0 when epoch is 0

---

## Implementation Notes

- Unlike `prune_before_height`, checkpoint pruning does not need to touch multiple column families, so a simple loop of individual deletes is acceptable. A `WriteBatch` may still be used for efficiency.
- No cache eviction is needed since checkpoints are not cached in memory.
- Consider batching deletes if the number of checkpoints to prune is large.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_prune_checkpoints_basic` | unit | Store checkpoints at epochs 1-10, prune before 5, verify epochs 1-4 removed, 5-10 intact |
| `test_prune_checkpoints_count` | unit | Store 10 checkpoints, prune before 6, verify return value is 5 |
| `test_prune_checkpoints_none_to_prune` | unit | Store at epochs 10, 20; prune before 5; verify 0 returned and all intact |
| `test_prune_checkpoints_zero_epoch` | unit | Prune before epoch 0, verify 0 returned |
| `test_prune_checkpoints_all` | unit | Store at epochs 1-5, prune before 100, verify all removed |
| `test_prune_checkpoints_sparse` | unit | Store at epochs 3, 7, 15, 42; prune before 10; verify epochs 3, 7 removed; 15, 42 intact |

---

## Expected Test Files

- `tests/unit/pruning/test_prn_002_prune_checkpoints_before_epoch.rs`
