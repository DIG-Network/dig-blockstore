# CKP-002: get_checkpoint

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 9.2 |

---

## Summary

`get_checkpoint` MUST read a `StoredCheckpoint` from `CF_CHECKPOINTS` using a big-endian u64 epoch key and deserialize it via bincode. Returns `None` if no checkpoint exists for the given epoch.

---

## Specification

```rust
/// Retrieve a checkpoint by epoch.
/// Returns None if no checkpoint is stored for the given epoch.
pub fn get_checkpoint(&self, epoch: u64) -> Result<Option<StoredCheckpoint>>
```

### Behavior

1. Encode `epoch` as a big-endian 8-byte key: `epoch.to_be_bytes()`.
2. Read from `CF_CHECKPOINTS` via `RocksDB::get_cf()`.
3. If no value is found, return `Ok(None)`.
4. If a value is found, deserialize with `bincode::deserialize()` into `StoredCheckpoint`.
5. Return `Ok(Some(checkpoint))` on successful deserialization.
6. Propagate RocksDB or bincode deserialization errors as `Result::Err`.

---

## Acceptance Criteria

- [ ] Epoch key is encoded as 8-byte big-endian u64
- [ ] Read targets `CF_CHECKPOINTS` column family
- [ ] Returns `Ok(None)` for a non-existent epoch
- [ ] Returns `Ok(Some(checkpoint))` for a stored epoch with all fields intact
- [ ] Deserialization uses bincode
- [ ] Corrupted data returns a deserialization error, not a panic

---

## Implementation Notes

- No caching layer exists for checkpoints; every call reads directly from RocksDB.
- The `StoredCheckpoint` type must implement `serde::Deserialize`.
- Consider `ReadOptions` with `set_verify_checksums(true)` for data integrity verification on read.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_get_checkpoint_existing` | unit | Store a checkpoint at epoch 42, retrieve it, verify all fields match |
| `test_get_checkpoint_missing` | unit | Query an epoch that was never stored, verify `None` returned |
| `test_get_checkpoint_after_overwrite` | unit | Store two checkpoints at same epoch, verify get returns the latest |
| `test_get_checkpoint_multiple_epochs` | unit | Store at epochs 10, 20, 30; retrieve each independently; verify no cross-contamination |
| `test_get_checkpoint_corrupted_data` | unit | Write invalid bytes directly to CF_CHECKPOINTS, verify deserialization error returned |

---

## Expected Test Files

- `tests/ckp_002_tests.rs`
