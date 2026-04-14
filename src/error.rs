//! `BlockStore` error surface (`BlockStoreError`).
//!
//! **Requirements**
//! - [`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) — thirteen
//!   variants, `thiserror::Error` + `Debug`.
//! - [`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md) —
//!   `From<rocksdb::Error>` (via `#[from]`), [`From<bincode::Error>`] for [`Serialization`](BlockStoreError::Serialization),
//!   and explicit zstd / [`std::io::Error`] → [`Compression`](BlockStoreError::Compression) mapping
//!   ([`BlockStoreError::compression_from_io`]).
//! - [`ERR-003`](../docs/requirements/domains/error_types/specs/ERR-003_error_display_messages.md) —
//!   every variant’s [`std::fmt::Display`] (via thiserror `#[error]`) must embed actionable context: hashes as
//!   hex ([`Bytes32`](chia_protocol::Bytes32) implements [`Display`](std::fmt::Display)), numeric fields inlined,
//!   and static messages for unit variants ([`NORMATIVE` ERR-003](../docs/requirements/domains/error_types/NORMATIVE.md#err-003-error-display-messages)).
//! - Normative: [`ERR domain NORMATIVE`](../docs/requirements/domains/error_types/NORMATIVE.md#err-001-blockstoreerror-enum).
//! - SPEC: [`SPEC.md` §12](../docs/resources/SPEC.md) (error taxonomy; ERR-001 adds `EmptyReorgChain` and
//!   `PipelineClosed` beyond the SPEC snippet).
//!
//! ## Operational errors without a first-class variant
//!
//! [`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) caps the enum at
//! thirteen cases. Some [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md) guards
//! (missing read-only path, read-only mutation, double genesis) therefore map to [`BlockStoreError::Serialization`]
//! with **stable string payloads** documented below so integration tests and future refactors can match
//! deterministically. If the taxonomy gains dedicated variants later, these constants become the migration
//! anchor.

use chia_protocol::Bytes32;
use thiserror::Error;

/// Stable [`BlockStoreError::Serialization`] payload prefix when [`crate::store::BlockStore::open_readonly`]
/// is called with a path that does not exist on disk.
pub const ERR_OPEN_READONLY_PATH_MISSING_PREFIX: &str =
    "open_readonly: database path does not exist: ";

/// Stable [`BlockStoreError::Serialization`] payload when [`crate::store::BlockStore::init_genesis`] runs on
/// a read-only handle.
pub const ERR_INIT_GENESIS_READ_ONLY: &str = "init_genesis: block store is read-only";

/// Stable [`BlockStoreError::Serialization`] payload when genesis metadata is already present.
pub const ERR_INIT_GENESIS_ALREADY_INITIALIZED: &str =
    "init_genesis: block store already initialized";

/// Crate-level error for persistence, chain, and I/O boundaries ([`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md)).
///
/// **Display:** Each `#[error("…")]` attribute is the contract for logs and user-facing text ([`ERR-003`](../docs/requirements/domains/error_types/specs/ERR-003_error_display_messages.md)).
///
/// **Async:** All variants are `Send + Sync` (see `test_err_001_enum_variants` static assertions).
#[derive(Debug, Error)]
pub enum BlockStoreError {
    /// Wraps an underlying RocksDB error ([`rocksdb::Error`]).
    ///
    /// **`#[source]`:** Preserves the inner error for `Error::source()` (ERR-001 test plan).
    #[error("rocksdb error: {0}")]
    RocksDb(
        #[from]
        #[source]
        rocksdb::Error,
    ),

    /// Bincode or other structural encode/decode failure ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md), [`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Zstd compress/decompress failure ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
    #[error("compression error: {0}")]
    Compression(String),

    /// Requested block hash is not present ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)).
    #[error("block not found: {0}")]
    BlockNotFound(Bytes32),

    /// No checkpoint row for the given epoch ([`CKP-002`](../docs/requirements/domains/checkpoint_storage/specs/CKP-002.md)).
    #[error("checkpoint not found for epoch {0}")]
    CheckpointNotFound(u64),

    /// Chain / canonical operation referenced a hash that is not stored ([`CAN-003`](../docs/requirements/domains/canonical_chain/specs/CAN-003.md)).
    #[error("block not in store: {0}")]
    BlockNotInStore(Bytes32),

    /// Rollback would violate the pruning floor ([`ROR-001`](../docs/requirements/domains/rollback_reorg/specs/ROR-001.md)).
    #[error("rollback target {target} is below minimum retained height {min}")]
    RollbackBelowMin { target: u64, min: u64 },

    /// Rollback target is above the current tip ([`ROR-001`](../docs/requirements/domains/rollback_reorg/specs/ROR-001.md)).
    #[error("rollback target {target} is above current tip {tip}")]
    RollbackAboveTip { target: u64, tip: u64 },

    /// Metadata does not contain a tip ([`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md)).
    #[error("no chain tip set")]
    NoTip,

    /// On-disk schema / version mismatch ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md) metadata keys).
    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    /// Operation requires genesis / initialized metadata ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md), [`BLK-013`](../docs/requirements/domains/block_storage/specs/BLK-013.md)).
    #[error("store not initialized")]
    NotInitialized,

    /// [`apply_reorg`](crate::store::BlockStore) called with an empty new chain ([`ROR-003`](../docs/requirements/domains/rollback_reorg/specs/ROR-003.md)).
    #[error("empty reorg chain: new_chain_hashes must not be empty")]
    EmptyReorgChain,

    /// Async write pipeline channel is closed ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)).
    #[error("write pipeline closed")]
    PipelineClosed,
}

impl BlockStoreError {
    /// Maps an [`std::io::Error`] produced by **zstd** compress/decompress helpers ([`zstd::encode_all`],
    /// [`zstd::decode_all`], …) into [`BlockStoreError::Compression`].
    ///
    /// **Rationale ([`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md)):**
    /// We intentionally **do not** implement [`From<std::io::Error>`] on [`BlockStoreError`]. Plain
    /// filesystem I/O (for example `create_dir_all`) is surfaced as [`Serialization`](BlockStoreError::Serialization)
    /// today ([`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) interim mapping);
    /// a blanket `From<io::Error>` would make those sites ambiguous or route the wrong variant. Callers
    /// that know the `io::Error` came from zstd should use this helper with [`Result::map_err`] or a closure.
    #[must_use]
    pub fn compression_from_io(err: std::io::Error) -> Self {
        Self::Compression(err.to_string())
    }
}

impl From<bincode::Error> for BlockStoreError {
    /// Bincode encode/decode failures map one-to-one onto [`BlockStoreError::Serialization`] ([`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md)).
    ///
    /// **Note:** [`std::io::Error`] is **not** covered here — use [`BlockStoreError::compression_from_io`] for zstd I/O.
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
