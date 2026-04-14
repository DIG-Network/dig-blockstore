# CKP-004: get_checkpoints_in_range

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 9.2 |

---

## Summary

`get_checkpoints_in_range` MUST use a RocksDB forward iterator to retrieve all checkpoints with epochs between `start_epoch` and `end_epoch` (inclusive). Returns an empty `Vec` if no checkpoints exist in the range.

---

## Specification

```rust
/// Retrieve all checkpoints within an epoch range (inclusive).
/// Returns an empty Vec if no checkpoints exist in the range.
pub fn get_checkpoints_in_range(
    &self,
    start_epoch: u64,
    end_epoch: u64,
) -> Result<Vec<StoredCheckpoint>>
```

### Behavior

1. Encode `start_epoch` as a big-endian 8-byte key.
2. Create a RocksDB forward iterator on `CF_CHECKPOINTS` and seek to the `start_epoch` key.
3. Iterate forward, for each entry:
   a. Decode the key as a big-endian u64 epoch.
   b. If `epoch > end_epoch`, stop iteration.
   c. Deserialize the value via `bincode::deserialize()` into `StoredCheckpoint`.
   d. Append to the result vector.
4. Return the collected `Vec<StoredCheckpoint>`.
5. If no entries fall within the range, return an empty `Vec`.

### Range Semantics

- The range is **inclusive** on both ends: `[start_epoch, end_epoch]`.
- If `start_epoch > end_epoch`, the result MUST be an empty `Vec` (no error).
- Gaps in epoch numbers are expected; only epochs that have stored checkpoints are returned.

---

## Acceptance Criteria

- [ ] Uses RocksDB iterator with seek to `start_epoch`
- [ ] Returns all checkpoints where `start_epoch <= epoch <= end_epoch`
- [ ] Returns empty `Vec` when no checkpoints exist in range
- [ ] Returns empty `Vec` when `start_epoch > end_epoch`
- [ ] Checkpoints are returned in ascending epoch order
- [ ] Does not scan entries outside the requested range
- [ ] Handles sparse epochs correctly (only returns stored checkpoints)

---

## Implementation Notes

- Use `iterator_cf()` with `IteratorMode::From(start_key, Direction::Forward)` to seek to the starting position.
- The iterator will naturally land on the first key >= `start_epoch` if the exact key does not exist.
- Break the loop when the decoded key exceeds `end_epoch` to avoid unnecessary iteration.
- Consider pre-allocating the result vector if an approximate count is known.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_range_all_included` | unit | Store at epochs 5, 10, 15; query [5, 15]; verify all three returned in order |
| `test_range_partial` | unit | Store at epochs 5, 10, 15, 20; query [8, 18]; verify epochs 10, 15 returned |
| `test_range_empty` | unit | Store at epochs 5, 10; query [20, 30]; verify empty Vec |
| `test_range_inverted` | unit | Query [15, 5]; verify empty Vec (no error) |
| `test_range_single_match` | unit | Store at epochs 5, 10, 15; query [10, 10]; verify only epoch 10 returned |
| `test_range_boundary_inclusive` | unit | Store at epochs 5, 10, 15; query [5, 15]; verify both boundaries included |
| `test_range_sparse_epochs` | unit | Store at epochs 1, 100, 200, 500; query [50, 250]; verify epochs 100, 200 returned |
| `test_range_empty_store` | unit | Query on empty store; verify empty Vec |

---

## Expected Test Files

- `tests/unit/checkpoint_storage/test_ckp_004_get_checkpoints_in_range.rs`
