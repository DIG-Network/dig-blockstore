//! `BlockStore` error surface (`BlockStoreError`).
//!
//! **Spec / requirements**
//! - [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md) — constructors and genesis (`RocksDb`, `ReadOnly`, `AlreadyInitialized`, …).
//! - Full taxonomy polish: [`ERR-001`…`ERR-003`](../docs/requirements/domains/error_types/NORMATIVE.md).

use std::path::PathBuf;

use thiserror::Error;

/// Top-level error type for persistent block storage operations.
#[derive(Debug, Error)]
pub enum BlockStoreError {
    /// Filesystem error creating/opening paths.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// RocksDB I/O or internal error ([`rocksdb::Error`]).
    #[error(transparent)]
    RocksDb(#[from] rocksdb::Error),

    /// Database directory does not exist (e.g. [`crate::store::BlockStore::open_readonly`]).
    #[error("database path does not exist: {0}")]
    PathDoesNotExist(PathBuf),

    /// Mutating API called on a read-only store ([`crate::store::BlockStore::open_readonly`]).
    #[error("operation requires a writable block store")]
    ReadOnly,

    /// Genesis or tip already present ([`crate::store::BlockStore::init_genesis`]).
    #[error("block store already initialized")]
    AlreadyInitialized,

    /// `bincode` encode/decode failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Malformed stored bytes (tip, hash payload, …).
    #[error("invalid stored data: {0}")]
    InvalidData(String),

    /// Zstd compress/decompress failure on block payloads ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
    #[error("zstd error: {0}")]
    Zstd(String),
}
