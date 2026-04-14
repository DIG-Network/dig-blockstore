# KEY-003: Epoch Keys (8 bytes, big-endian)

## Summary

Epoch keys used in `CF_CHECKPOINTS` MUST be `u64` values encoded as 8-byte big-endian byte arrays, following the same pattern as height keys. This ensures ascending lexicographic order matches numeric epoch order.

## Specification

The key encoding function for epoch-based column families converts a `u64` epoch to its 8-byte big-endian representation:

```rust
/// Encode an epoch number as a RocksDB key for CF_CHECKPOINTS.
/// Returns an 8-byte big-endian representation ensuring lexicographic
/// order matches numeric order.
pub fn encode_epoch_key(epoch: u64) -> [u8; 8] {
    epoch.to_be_bytes()
}

/// Decode an epoch key back to a u64.
pub fn decode_epoch_key(key: &[u8; 8]) -> u64 {
    u64::from_be_bytes(*key)
}
```

- The key MUST be exactly 8 bytes in length.
- The encoding is identical to height keys (KEY-002) since both are `u64` big-endian.
- Separate functions are provided for epochs and heights to maintain semantic clarity and allow independent evolution.
- Big-endian encoding guarantees that for any two epochs `a < b`, the byte representation of `a` is lexicographically less than that of `b`.

## Acceptance Criteria

1. `encode_epoch_key` returns exactly 8 bytes for any `u64` input.
2. Big-endian encoding: `encode_epoch_key(1)` produces `[0, 0, 0, 0, 0, 0, 0, 1]`.
3. Sort order: for all `a < b`, `encode_epoch_key(a) < encode_epoch_key(b)` under bytewise comparison.
4. Round-trip: `decode_epoch_key(&encode_epoch_key(e)) == e` for any `u64` e.

## Implementation Notes

- While the encoding is identical to height keys, the functions are kept separate so that:
  - Type safety can be added later (e.g., newtype wrappers for `Height` vs `Epoch`).
  - Callers clearly communicate intent at the call site.
- Epochs in the dig protocol represent fixed-height intervals used for checkpoint creation.

## Test Plan

1. **Zero epoch**: `encode_epoch_key(0)` produces 8 zero bytes.
2. **Epoch 1**: `encode_epoch_key(1)` produces `[0,0,0,0,0,0,0,1]`.
3. **Max epoch**: `encode_epoch_key(u64::MAX)` produces 8 `0xFF` bytes.
4. **Sort order**: Encode epochs `[0, 1, 10, 100, 1000, u64::MAX]`, verify bytewise sort matches numeric sort.
5. **Round-trip**: Encode and decode a range of epochs, assert equality.
6. **Consistency with height encoding**: For any value `v`, verify `encode_epoch_key(v) == encode_height_key(v)` (same encoding, different semantics).

## Expected Test Files

- `tests/unit/key_encoding/test_key_003_epoch_keys.rs`
