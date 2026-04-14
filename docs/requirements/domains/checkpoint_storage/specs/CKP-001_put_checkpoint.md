# CKP-001: put_checkpoint

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 9.1 |

---

## Summary

`put_checkpoint` MUST serialize a `StoredCheckpoint` via bincode and persist it to `CF_CHECKPOINTS` keyed by the checkpoint's epoch encoded as a big-endian u64. The operation is idempotent: storing a checkpoint for an epoch that already exists overwrites the previous value.

---

## Specification

```rust
/// Persist a checkpoint to the checkpoint column family.
/// Overwrites any existing checkpoint at the same epoch.
pub fn put_checkpoint(&self, checkpoint: &StoredCheckpoint) -> Result<()>
```

### Behavior

1. Extract the epoch from `checkpoint.epoch`.
2. Encode the epoch as a big-endian 8-byte key: `epoch.to_be_bytes()`.
3. Serialize `checkpoint` using `bincode::serialize()`.
4. Write the serialized bytes to `CF_CHECKPOINTS` with the epoch key via `RocksDB::put_cf()`.
5. If a checkpoint already exists at the same epoch, the write silently overwrites it (idempotent).
6. Return `Ok(())` on success, or propagate the RocksDB/bincode error.

### Key Encoding

The epoch key uses big-endian encoding to ensure that RocksDB's lexicographic byte ordering matches numeric epoch ordering. This is critical for correct iterator behavior in `get_latest_checkpoint` and `get_checkpoints_in_range`.

---

## Acceptance Criteria

- [ ] `StoredCheckpoint` is serialized via bincode (not JSON, not custom encoding)
- [ ] Key is the epoch encoded as 8-byte big-endian u64
- [ ] Write targets `CF_CHECKPOINTS` column family
- [ ] Overwriting an existing epoch does not return an error
- [ ] Round-trip: `put_checkpoint` followed by `get_checkpoint` returns an equal `StoredCheckpoint`
- [ ] RocksDB errors are propagated as `Result::Err`

---

## Implementation Notes

- The `StoredCheckpoint` type must implement `serde::Serialize` and `serde::Deserialize`.
- Big-endian encoding is consistent with the epoch key encoding used in `KEY-003`.
- No caching layer is used for checkpoints; all reads go directly to RocksDB.
- Consider using `WriteOptions` with `set_sync(false)` for performance if checkpoints are written in bulk (e.g., during initial sync). Durability is ensured by the WAL.

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_put_checkpoint_round_trip` | unit | Store a checkpoint, retrieve it by epoch, verify all fields match |
| `test_put_checkpoint_overwrite` | unit | Store two different checkpoints at the same epoch, verify the second overwrites the first |
| `test_put_checkpoint_key_encoding` | unit | Verify the raw RocksDB key is 8-byte big-endian epoch |
| `test_put_checkpoint_bincode_format` | unit | Verify the raw value bytes match `bincode::serialize` output |
| `test_put_checkpoint_multiple_epochs` | unit | Store checkpoints at epochs 1, 100, 1000; retrieve each independently |

---

## Expected Test Files

- `tests/ckp_001_tests.rs`
