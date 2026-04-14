# CKP-003: get_latest_checkpoint

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 9.2 |

---

## Summary

`get_latest_checkpoint` MUST use a RocksDB reverse iterator on `CF_CHECKPOINTS` to find the checkpoint with the highest epoch. Returns `None` if no checkpoints are stored.

---

## Specification

```rust
/// Retrieve the most recent checkpoint (highest epoch).
/// Returns None if no checkpoints exist in the store.
pub fn get_latest_checkpoint(&self) -> Result<Option<StoredCheckpoint>>
```

### Behavior

1. Create a RocksDB iterator on `CF_CHECKPOINTS` in reverse direction (seeking to the end).
2. Read the first entry from the reverse iterator (this is the entry with the lexicographically largest key, which corresponds to the highest epoch due to big-endian encoding).
3. If the iterator is exhausted (no entries), return `Ok(None)`.
4. Deserialize the value via `bincode::deserialize()` into `StoredCheckpoint`.
5. Return `Ok(Some(checkpoint))`.

### Why Reverse Iterator

Because epoch keys are stored as big-endian u64, RocksDB's lexicographic ordering matches numeric ordering. A reverse iterator starting from the end immediately yields the highest epoch without scanning the entire column family.

---

## Acceptance Criteria

- [ ] Uses a reverse iterator on `CF_CHECKPOINTS`
- [ ] Returns the checkpoint with the highest epoch value
- [ ] Returns `Ok(None)` when no checkpoints are stored
- [ ] After adding a new checkpoint with a higher epoch, returns the new checkpoint
- [ ] Does not scan the entire column family (O(1) seek, not O(n) scan)

---

## Implementation Notes

- Use `DBIteratorWithThreadMode::new_cf()` with `IteratorMode::End` to create the reverse iterator, then call `.next()` once.
- Alternatively, use `seek_to_last()` on a forward iterator and read the single entry.
- The big-endian key encoding is essential for correctness: if keys were little-endian, the lexicographic max would not correspond to the numeric max.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_get_latest_checkpoint_empty` | unit | Query on empty store, verify `None` returned |
| `test_get_latest_checkpoint_single` | unit | Store one checkpoint, verify it is returned as latest |
| `test_get_latest_checkpoint_multiple` | unit | Store checkpoints at epochs 5, 10, 15; verify epoch 15 returned |
| `test_get_latest_checkpoint_after_insert` | unit | Store at epochs 5, 10; get latest (10); store at epoch 20; get latest (20) |
| `test_get_latest_checkpoint_non_sequential` | unit | Store at epochs 100, 3, 50, 999; verify epoch 999 returned |

---

## Expected Test Files

- `tests/unit/checkpoint_storage/test_ckp_003_get_latest_checkpoint.rs`
