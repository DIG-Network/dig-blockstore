# KEY-004: Metadata Keys (variable UTF-8)

## Summary

Metadata keys used in `CF_METADATA` MUST be UTF-8 encoded strings. Unlike hash and height/epoch keys, metadata keys are variable-length. The well-known metadata key names are defined as constants.

## Specification

The key encoding for metadata column family uses raw UTF-8 byte sequences. The public API name is **`metadata_key`** (crate-root re-export per [`STR-003`](../../crate_structure/specs/STR-003.md)); the snippet below matches KEY-004 semantics.

```rust
/// Well-known metadata key constants.
pub const META_TIP: &str = "tip";
pub const META_GENESIS_HASH: &str = "genesis_hash";
pub const META_MIN_HEIGHT: &str = "min_height";
pub const META_SCHEMA_VERSION: &str = "schema_version";
pub const META_ZSTD_DICT: &str = "zstd_dict";

/// Encode a metadata key name as a RocksDB key for CF_METADATA.
/// Returns the raw UTF-8 byte representation.
pub fn metadata_key(name: &str) -> &[u8] {
    name.as_bytes()
}
```

- Keys are variable-length byte sequences corresponding to the UTF-8 encoding of the key name.
- No null terminator or length prefix is added.
- All well-known metadata keys use ASCII-only characters, but the encoding supports the full UTF-8 range.
- The well-known keys MUST be:
  - `"tip"` &mdash; serialized `ChainTip` struct (current chain tip)
  - `"genesis_hash"` &mdash; `Bytes32` genesis block hash
  - `"min_height"` &mdash; `u64` minimum retained height (pruning boundary)
  - `"schema_version"` &mdash; `u32` store schema version
  - `"zstd_dict"` &mdash; trained Zstd dictionary bytes

## Acceptance Criteria

1. `metadata_key("tip")` returns `[0x74, 0x69, 0x70]` (UTF-8 for "tip").
2. All well-known metadata key constants are valid UTF-8 strings.
3. Keys are variable-length: different key names produce different-length byte sequences.
4. No null terminator is present in the encoded key.
5. All five well-known metadata key constants are defined and accessible.

## Implementation Notes

- Metadata keys are few in number (5 well-known keys) and accessed by exact match, not range scan. Sort order is not critical for this column family.
- Using string keys provides human-readable debugging when inspecting the RocksDB store with tools like `ldb`.
- The `metadata_key` function is effectively a zero-cost wrapper around `str::as_bytes()`.

## Test Plan

1. **Known key encoding**: Encode each of the 5 well-known keys, verify exact UTF-8 byte sequences.
2. **Variable length**: Verify `metadata_key("tip").len() == 3` and `metadata_key("schema_version").len() == 14`.
3. **No null terminator**: Verify the last byte of each encoded key is not `0x00`.
4. **Uniqueness**: Verify all 5 well-known key encodings are distinct.
5. **Constants defined**: Verify `META_TIP`, `META_GENESIS_HASH`, `META_MIN_HEIGHT`, `META_SCHEMA_VERSION`, and `META_ZSTD_DICT` are defined with expected string values.

## Expected Test Files

- `tests/test_key_004_metadata_keys.rs`
