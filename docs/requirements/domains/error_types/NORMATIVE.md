# Error Types - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Error Types |
| **Prefix** | ERR |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### ERR-001: BlockStoreError Enum

`BlockStoreError` **MUST** define the following variants:

- `RocksDb(rocksdb::Error)` &mdash; wraps underlying RocksDB errors
- `Serialization(String)` &mdash; bincode or other serialization failures
- `Compression(String)` &mdash; zstd compression/decompression failures
- `BlockNotFound(Bytes32)` &mdash; requested block hash not present in store
- `CheckpointNotFound(u64)` &mdash; requested checkpoint epoch not present
- `BlockNotInStore(Bytes32)` &mdash; block referenced but not in store (e.g., during chain operations)
- `RollbackBelowMin { target: u64, min: u64 }` &mdash; rollback target is below minimum retained height
- `RollbackAboveTip { target: u64, tip: u64 }` &mdash; rollback target is above current chain tip
- `NoTip` &mdash; store has no chain tip (uninitialized or corrupted)
- `SchemaMismatch { expected: u32, found: u32 }` &mdash; store schema version does not match expected version
- `NotInitialized` &mdash; store has not been initialized with a genesis block

`BlockStoreError` **MUST** derive `thiserror::Error` and `Debug`.

**Spec reference:** SPEC Section 12

---

### ERR-002: Error From Conversions

`BlockStoreError` **MUST** implement `From<rocksdb::Error>` for the `RocksDb` variant. Serialization errors from `bincode` **MUST** be mapped to the `Serialization(String)` variant. Zstd errors **MUST** be mapped to the `Compression(String)` variant.

These conversions enable use of the `?` operator throughout the codebase for ergonomic error propagation.

**Spec reference:** SPEC Section 12

---

### ERR-003: Error Display Messages

All `BlockStoreError` variants **MUST** produce meaningful `Display` messages via `thiserror` `#[error("...")]` attributes. Messages **MUST** include relevant context:

- `RocksDb`: the underlying RocksDB error message
- `Serialization`: the serialization error description
- `Compression`: the compression error description
- `BlockNotFound`: the block hash (hex-encoded)
- `CheckpointNotFound`: the epoch number
- `BlockNotInStore`: the block hash (hex-encoded)
- `RollbackBelowMin`: both target and min heights
- `RollbackAboveTip`: both target and tip heights
- `NoTip`: descriptive message (no additional context needed)
- `SchemaMismatch`: both expected and found schema versions
- `NotInitialized`: descriptive message (no additional context needed)

**Spec reference:** SPEC Section 12
