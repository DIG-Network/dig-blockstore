# KEY-002: Height Keys (8 bytes, big-endian)

## Summary

Height keys used in `CF_CANONICAL` MUST be `u64` values encoded as 8-byte big-endian byte arrays. Big-endian encoding ensures that RocksDB's default bytewise comparator produces ascending numeric order during iteration.

## Specification

The key encoding function for height-based column families converts a `u64` height to its 8-byte big-endian representation:

```rust
/// Encode a block height as a RocksDB key for CF_CANONICAL.
/// Returns an 8-byte big-endian representation ensuring lexicographic
/// order matches numeric order.
pub fn encode_height_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}

/// Decode a height key back to a u64.
pub fn decode_height_key(key: &[u8; 8]) -> u64 {
    u64::from_be_bytes(*key)
}
```

- The key MUST be exactly 8 bytes in length.
- Big-endian encoding guarantees that for any two heights `a < b`, the byte representation of `a` is lexicographically less than that of `b`.
- This property is critical for RocksDB range scans and ordered iteration over `CF_CANONICAL`.

### Sort Order Guarantee

| Height (decimal) | Key (hex) |
|-------------------|-----------|
| 0 | `00 00 00 00 00 00 00 00` |
| 1 | `00 00 00 00 00 00 00 01` |
| 255 | `00 00 00 00 00 00 00 FF` |
| 256 | `00 00 00 00 00 00 01 00` |
| 1000 | `00 00 00 00 00 00 03 E8` |
| u64::MAX | `FF FF FF FF FF FF FF FF` |

## Acceptance Criteria

1. `encode_height_key` returns exactly 8 bytes for any `u64` input.
2. Big-endian encoding: `encode_height_key(1)` produces `[0, 0, 0, 0, 0, 0, 0, 1]`.
3. Sort order: for all `a < b`, `encode_height_key(a) < encode_height_key(b)` under bytewise comparison.
4. Round-trip: `decode_height_key(&encode_height_key(h)) == h` for any `u64` h.

## Implementation Notes

- `u64::to_be_bytes()` is a `const fn` and compiles to a single `bswap` instruction on little-endian architectures.
- Little-endian encoding would violate the sort order requirement: height 256 (`0x00 0x01 ...`) would sort before height 1 (`0x01 0x00 ...`) under bytewise comparison.
- The 8-byte fixed-size key allows RocksDB to use fixed-size key optimizations.

## Test Plan

1. **Zero height**: `encode_height_key(0)` produces 8 zero bytes.
2. **Height 1**: `encode_height_key(1)` produces `[0,0,0,0,0,0,0,1]`.
3. **Max height**: `encode_height_key(u64::MAX)` produces 8 `0xFF` bytes.
4. **Sort order**: Encode heights `[0, 1, 2, 255, 256, 1000, u64::MAX]`, verify bytewise sort matches numeric sort.
5. **Round-trip**: Encode and decode a range of heights, assert equality.
6. **Negative test**: Verify that little-endian encoding would break sort order (document why big-endian is required).

## Expected Test Files

- `tests/unit/key_encoding/test_key_002_height_keys.rs`
