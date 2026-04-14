# KEY-001: Hash Keys (32 bytes)

## Summary

Hash keys used in `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED` column families MUST be raw 32-byte `Bytes32` values with no prefix, framing, or additional encoding.

## Specification

The key encoding function for hash-based column families takes a `Bytes32` hash and returns the raw 32-byte slice directly:

```rust
use chia_protocol::Bytes32;

/// Public API: `dig_blockstore::hash_key` in `src/encoding.rs`.
/// Returns the raw 32-byte representation with no prefix or framing.
pub fn hash_key(hash: &Bytes32) -> &[u8; 32] {
    hash.as_ref().try_into().expect("Bytes32 is 32 bytes")
}
```

- The key MUST be exactly 32 bytes in length.
- No length prefix, type tag, or other framing is applied.
- The same encoding is used for all three hash-keyed column families: `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED`.
- The `Bytes32` type is a 32-byte fixed-size array wrapper from `dig-block`.

## Acceptance Criteria

1. `hash_key` returns a 32-byte array reference for any valid `Bytes32` input.
2. The returned bytes are identical to the raw bytes of the input hash.
3. No additional bytes are prepended or appended to the key.
4. Round-trip: a `Bytes32` constructed from the key bytes is equal to the original hash.

_(Historical spec text referred to `encode_hash_key`; the public API is `hash_key` in `src/encoding.rs`, re-exported at the crate root per STR-003.)_

## Implementation Notes

- `Bytes32` implements `AsRef<[u8; 32]>`, so the encoding is zero-copy.
- Fixed 32-byte keys give RocksDB predictable key-size distribution, aiding bloom filter and block index efficiency.
- Hash keys do not have a meaningful sort order since hash values are effectively random.

## Test Plan

1. **Zero hash**: Encode `Bytes32::default()` (all zeros), verify key is 32 zero bytes.
2. **Known hash**: Encode a deterministic hash value, verify exact byte match.
3. **Length assertion**: Verify `encode_hash_key` output length is always exactly 32.
4. **Round-trip**: Convert key bytes back to `Bytes32`, assert equality with original.
5. **Distinct hashes produce distinct keys**: Encode two different hashes, assert keys differ.

## Expected Test Files

- `tests/unit/key_encoding/test_key_001_hash_keys.rs`
