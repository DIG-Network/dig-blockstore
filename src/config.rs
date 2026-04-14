//! `BlockStoreConfig` — paths, cache sizes, RocksDB tuning hooks.
//!
//! **Requirements**
//! - [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md),
//!   [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md),
//!   [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) (test helper field set)
//! - Target field set / defaults: [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)
//!
//! ## Naming (`db_path` vs `path`)
//!
//! The authoritative storage spec ([`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md))
//! names this field `path`. This crate historically exposed **`db_path`** at the Rust boundary; we keep that
//! identifier so [`crate::store::BlockStore::open`](crate::store::BlockStore::open) call sites stay stable.
//! Semantically it is the RocksDB directory root (same as SPEC §3.6 / TYP-008 `path`).
//!
//! ## Defaults vs TYP-008
//!
//! Numeric defaults align with [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)
//! where applicable. [`warm_cache_on_open`](BlockStoreConfig::warm_cache_on_open) remains **`false`** in
//! [`Default`] until cache warming is fully integrated across tests (STR-004 already overrides when needed);
//! TYP-008 lists `true` as the production-oriented default—see tracking for TYP-008 completion.
//!
//! **Rationale:** `BlockStore::open` currently applies only `db_path`, warm-cache flags, and depth; other
//! fields are carried for API completeness (STR-005 / toward TYP-008) and will wire into RocksDB options
//! in later requirements (TYP-002 / TYP-003 / BLK-008).

use std::path::PathBuf;

/// Configuration for opening or creating a [`crate::store::BlockStore`].
///
/// **See:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) `test_config`,
/// [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md) for the full intended surface.
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    // --- Storage path (TYP-008 `path`; see module docs) ---
    /// Root directory for the RocksDB database files.
    pub db_path: PathBuf,

    // --- In-memory blockstore caches (CAC-001 / CAC-002 precursors) ---
    /// Max blocks retained in the sharded block cache.
    pub block_cache_capacity: usize,

    /// Max headers retained in the sharded header cache.
    pub header_cache_capacity: usize,

    /// Shard count for block/header caches; must be a power of two when enforced ([`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)).
    pub cache_shards: usize,

    /// When true, [`crate::store::BlockStore::open`] preloads recent canonical blocks ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md), [`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub warm_cache_on_open: bool,

    /// Trailing heights (inclusive) to touch when warming, starting from tip ([`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub warm_cache_depth: u64,

    // --- RocksDB tuning (TYP-002 / TYP-003 precursors) ---
    /// RocksDB memtable / write buffer budget per column family (bytes).
    pub write_buffer_size: usize,

    /// Shared block cache for RocksDB (bytes).
    pub block_cache_size: usize,

    /// `max_open_files` passed to RocksDB options.
    pub max_open_files: i32,

    /// When true, enable BlobDB-style large-value handling for block bodies ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).
    pub enable_blob_db: bool,

    /// When true, store compressed block payloads ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)); tests often disable for simplicity ([`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md)).
    pub compress_blocks: bool,

    /// Zstd level for block compression when `compress_blocks` is true.
    pub compression_level: i32,

    /// Whether to use a trained zstd dictionary ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).
    pub use_compression_dict: bool,

    // --- Write pipeline (BLK-008 precursor) ---
    /// Max blocks batched before a pipeline flush.
    pub write_pipeline_batch_size: usize,

    /// Max wait before flushing a partial pipeline batch (milliseconds).
    pub write_pipeline_flush_ms: u64,

    /// Bounded async channel capacity feeding the write pipeline.
    pub write_pipeline_channel_capacity: usize,

    /// When true, sync the WAL after each write (durability vs throughput).
    pub sync_writes: bool,

    /// Hint for sequential readahead ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).
    pub readahead_size: usize,

    // --- Pruning (PRN-003 / PRN-004 precursors) ---
    /// Register compaction-time pruning when true ([`PRN-003`](../docs/requirements/domains/pruning/specs/PRN-003_compaction_filter.md)).
    pub enable_compaction_pruning: bool,

    /// Optional floor height below which compaction may drop data; `None` disables.
    pub min_retained_height: Option<u64>,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("dig_blockstore_data"),
            block_cache_capacity: 1000,
            header_cache_capacity: 2000,
            cache_shards: 16,
            warm_cache_on_open: false,
            warm_cache_depth: 64,
            write_buffer_size: 67_108_864,
            block_cache_size: 134_217_728,
            max_open_files: 1000,
            enable_blob_db: true,
            compress_blocks: true,
            compression_level: 3,
            use_compression_dict: true,
            write_pipeline_batch_size: 64,
            write_pipeline_flush_ms: 100,
            write_pipeline_channel_capacity: 256,
            sync_writes: false,
            readahead_size: 2_097_152,
            enable_compaction_pruning: false,
            min_retained_height: None,
        }
    }
}
