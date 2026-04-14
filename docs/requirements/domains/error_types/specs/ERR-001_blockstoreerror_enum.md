# ERR-001: BlockStoreError Enum

## Summary

`BlockStoreError` is the crate-level error type for `dig-blockstore`. It MUST define exactly 13 variants covering all failure modes: RocksDB errors, serialization, compression, missing data, rollback violations, schema mismatches, uninitialized state, empty reorg chain, and pipeline closed. It MUST derive `thiserror::Error` and `Debug`.

## Specification

```rust
use thiserror::Error;
use dig_block::Bytes32;

#[derive(Debug, Error)]
pub enum BlockStoreError {
    /// Wraps an underlying RocksDB error.
    #[error("rocksdb error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    /// Serialization (bincode) failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Zstd compression or decompression failure.
    #[error("compression error: {0}")]
    Compression(String),

    /// Block with the given hash was not found in the store.
    #[error("block not found: {0}")]
    BlockNotFound(Bytes32),

    /// Checkpoint for the given epoch was not found.
    #[error("checkpoint not found for epoch {0}")]
    CheckpointNotFound(u64),

    /// Block referenced but not present in the store.
    #[error("block not in store: {0}")]
    BlockNotInStore(Bytes32),

    /// Rollback target height is below the minimum retained height.
    #[error("rollback target {target} is below minimum retained height {min}")]
    RollbackBelowMin { target: u64, min: u64 },

    /// Rollback target height is above the current chain tip.
    #[error("rollback target {target} is above current tip {tip}")]
    RollbackAboveTip { target: u64, tip: u64 },

    /// Store has no chain tip set.
    #[error("no chain tip set")]
    NoTip,

    /// Store schema version does not match expected version.
    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    /// Store has not been initialized with a genesis block.
    #[error("store not initialized")]
    NotInitialized,

    /// apply_reorg was called with an empty new_chain_hashes slice.
    #[error("empty reorg chain: new_chain_hashes must not be empty")]
    EmptyReorgChain,

    /// The write pipeline channel has been shut down.
    #[error("write pipeline closed")]
    PipelineClosed,
}
```

### Variant Summary

| Variant | Inner Data | Use Case |
|---------|-----------|----------|
| `RocksDb` | `rocksdb::Error` | Any RocksDB operation failure |
| `Serialization` | `String` | bincode encode/decode failure |
| `Compression` | `String` | zstd compress/decompress failure |
| `BlockNotFound` | `Bytes32` | Get/query for nonexistent block |
| `CheckpointNotFound` | `u64` | Get/query for nonexistent checkpoint epoch |
| `BlockNotInStore` | `Bytes32` | Chain operation references missing block |
| `RollbackBelowMin` | `{ target: u64, min: u64 }` | Rollback would go below pruning boundary |
| `RollbackAboveTip` | `{ target: u64, tip: u64 }` | Rollback target exceeds chain height |
| `NoTip` | (unit) | Operations requiring tip on uninitialized store |
| `SchemaMismatch` | `{ expected: u32, found: u32 }` | Opening store with incompatible schema |
| `NotInitialized` | (unit) | Operations requiring genesis on fresh store |
| `EmptyReorgChain` | (unit) | `apply_reorg` called with empty `new_chain_hashes` |
| `PipelineClosed` | (unit) | Write pipeline channel shut down |

## Acceptance Criteria

1. [x] `BlockStoreError` defines exactly 13 variants as listed above.
2. [x] The enum derives `Debug` and `thiserror::Error`.
3. [x] The enum implements `std::error::Error` (via thiserror).
4. [x] The enum implements `std::fmt::Display` (via thiserror `#[error]` attributes).
5. [x] Each variant can be constructed with the documented inner data types.
6. [x] The enum is `Send + Sync` (required for async error propagation).

## Implementation Notes

- `thiserror::Error` provides automatic `Display`, `Error`, and optionally `From` implementations.
- `Bytes32` must implement `Display` (typically hex-encoded) for the `#[error]` format strings to work.
- The `#[from]` attribute on `RocksDb` generates `impl From<rocksdb::Error> for BlockStoreError`.
- Struct variants (`RollbackBelowMin`, `RollbackAboveTip`, `SchemaMismatch`) use named fields for clarity.

## Test Plan

1. **Construct all variants**: Create an instance of each variant, verifying no compile errors.
2. **Debug trait**: Format each variant with `{:?}`, verify non-empty output.
3. **Error trait**: Call `.source()` on each variant, verify `RocksDb` returns `Some`, others return `None` or appropriate source.
4. **Send + Sync**: Static assert that `BlockStoreError: Send + Sync`.
5. **Exhaustive match**: Write a match expression covering all 13 variants to verify completeness.

## Expected Test Files

- `tests/err_001_tests.rs`
