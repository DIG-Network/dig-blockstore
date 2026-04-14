# ERR-003: Error Display Messages

## Summary

All `BlockStoreError` variants MUST produce meaningful `Display` messages via `thiserror` `#[error("...")]` attributes. Each message MUST include all relevant context data so that error logs and user-facing messages are actionable without additional debugging.

## Specification

The `Display` implementation is driven by `thiserror` `#[error]` attributes on each variant:

```rust
#[derive(Debug, Error)]
pub enum BlockStoreError {
    #[error("rocksdb error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("compression error: {0}")]
    Compression(String),

    #[error("block not found: {0}")]
    BlockNotFound(Bytes32),

    #[error("checkpoint not found for epoch {0}")]
    CheckpointNotFound(u64),

    #[error("block not in store: {0}")]
    BlockNotInStore(Bytes32),

    #[error("rollback target {target} is below minimum retained height {min}")]
    RollbackBelowMin { target: u64, min: u64 },

    #[error("rollback target {target} is above current tip {tip}")]
    RollbackAboveTip { target: u64, tip: u64 },

    #[error("no chain tip set")]
    NoTip,

    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    #[error("store not initialized")]
    NotInitialized,
}
```

### Expected Display Output Examples

| Variant | Example Input | Expected Display Output |
|---------|--------------|------------------------|
| `RocksDb` | IO error from RocksDB | `"rocksdb error: IO error: ..."` |
| `Serialization` | `"invalid byte at offset 5"` | `"serialization error: invalid byte at offset 5"` |
| `Compression` | `"dictionary mismatch"` | `"compression error: dictionary mismatch"` |
| `BlockNotFound` | hash `0xabcd...0001` | `"block not found: abcd...0001"` |
| `CheckpointNotFound` | epoch `42` | `"checkpoint not found for epoch 42"` |
| `BlockNotInStore` | hash `0xdead...beef` | `"block not in store: dead...beef"` |
| `RollbackBelowMin` | target=50, min=100 | `"rollback target 50 is below minimum retained height 100"` |
| `RollbackAboveTip` | target=200, tip=150 | `"rollback target 200 is above current tip 150"` |
| `NoTip` | (unit) | `"no chain tip set"` |
| `SchemaMismatch` | expected=2, found=1 | `"schema mismatch: expected 2, found 1"` |
| `NotInitialized` | (unit) | `"store not initialized"` |

## Acceptance Criteria

1. [x] Every variant produces a non-empty `Display` string.
2. [x] `RocksDb` display includes the underlying RocksDB error message.
3. [x] `Serialization` display includes the error description string.
4. [x] `Compression` display includes the error description string.
5. [x] `BlockNotFound` display includes the block hash.
6. [x] `CheckpointNotFound` display includes the epoch number.
7. [x] `BlockNotInStore` display includes the block hash.
8. [x] `RollbackBelowMin` display includes both target and min values.
9. [x] `RollbackAboveTip` display includes both target and tip values.
10. [x] `SchemaMismatch` display includes both expected and found versions.
11. [x] `NoTip` and `NotInitialized` produce descriptive static messages.
12. [x] `EmptyReorgChain` and `PipelineClosed` produce descriptive static messages ([`ERR-001`](ERR-001_blockstoreerror_enum.md) / NORMATIVE).

## Implementation Notes

- `thiserror` `#[error("...")]` uses `std::fmt::Display` formatting internally.
- `Bytes32` must implement `Display` to be interpolated in format strings. Typically this is hex-encoded output.
- Struct variant fields are referenced by name in the format string (e.g., `{target}`, `{min}`).
- Tuple variant fields are referenced by position (e.g., `{0}`).
- The `#[from]` attribute on `RocksDb` sets `rocksdb::Error` as the `source()`, but `{0}` in the error string still formats the inner error via its `Display` impl.

## Test Plan

1. **All variants non-empty**: Construct each variant, call `.to_string()`, assert result is not empty.
2. **Context inclusion for BlockNotFound**: Create with a known hash, verify the display string contains the hex representation of that hash.
3. **Context inclusion for CheckpointNotFound**: Create with epoch `42`, verify display contains `"42"`.
4. **Context inclusion for RollbackBelowMin**: Create with target=50, min=100, verify display contains both `"50"` and `"100"`.
5. **Context inclusion for RollbackAboveTip**: Create with target=200, tip=150, verify display contains both `"200"` and `"150"`.
6. **Context inclusion for SchemaMismatch**: Create with expected=2, found=1, verify display contains both `"2"` and `"1"`.
7. **Static messages**: Verify `NoTip` and `NotInitialized` produce expected exact strings.
8. **Serialization context**: Create with `"test error msg"`, verify display contains `"test error msg"`.
9. **Compression context**: Create with `"zstd failure"`, verify display contains `"zstd failure"`.

## Expected Test Files

- `tests/unit/error_types/test_err_003_display_messages.rs`
