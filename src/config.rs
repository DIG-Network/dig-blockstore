//! [`BlockStoreConfig`] — paths, cache sizes, RocksDB tuning, pipeline, and pruning hooks.
//!
//! **Requirements trace**
//! - [`TYP-008`](../../docs/requirements/domains/storage_types/specs/TYP-008.md) — field set, production defaults, manual [`Default`] (non-empty [`PathBuf`](std::path::PathBuf))
//! - [`NORMATIVE` TYP-008](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-008-blockstoreconfig-struct)
//! - [`STR-002`](../../docs/requirements/domains/crate_structure/specs/STR-002.md), [`STR-004`](../../docs/requirements/domains/crate_structure/specs/STR-004.md), [`STR-005`](../../docs/requirements/domains/crate_structure/specs/STR-005.md) (`test_config` overrides)
//! - Shared numeric tunables: [`TYP-002`](../../docs/requirements/domains/storage_types/specs/TYP-002.md) via [`crate::constants`] (`DEFAULT_*`, [`ZSTD_COMPRESSION_LEVEL`](crate::constants::ZSTD_COMPRESSION_LEVEL))
//!
//! ## Defaults and `Default` impl
//!
//! [`Default::default`] follows TYP-008 for the **core** knobs (path, caches, RocksDB, zstd, pipeline flags,
//! pruning flags). Numeric write-buffer / block-cache / cache capacities reuse [`crate::constants`] so
//! TYP-002 and TYP-008 stay numerically identical ([`tests/typ_002_metadata_keys.rs`](../../tests/typ_002_metadata_keys.rs)).
//!
//! **Why manual `Default`:** `#[derive(Default)]` would set `path` to an empty [`PathBuf`]; the spec requires a
//! conventional relative layout (`data/blockstore`) suitable for local dev and examples.
//!
//! ## Extension fields (beyond TYP-008’s core table)
//!
//! The crate also carries forward-looking fields wired by [`crate::store::BlockStore::open`] and future BLK/CAC work:
//!
//! - **`warm_cache_depth`** — how many trailing canonical heights to touch when [`warm_cache_on_open`] is true ([`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
//! - **`write_pipeline_channel_capacity`** — bounded queue depth for [`BLK-008`](../../docs/requirements/domains/block_storage/specs/BLK-008.md).
//! - **`readahead_size`** — sequential read hint ([`BLK-006`](../../docs/requirements/domains/block_storage/specs/BLK-006.md)).
//!
//! These are **not** duplicated in the short TYP-008 markdown table but are part of the public Rust API and are
//! covered by [`tests/typ_008_config.rs`](../../tests/typ_008_config.rs).

use std::path::PathBuf;

use crate::constants::{
    DEFAULT_BLOCK_CACHE_CAPACITY, DEFAULT_BLOCK_CACHE_SIZE, DEFAULT_HEADER_CACHE_CAPACITY,
    DEFAULT_MAX_OPEN_FILES, DEFAULT_WRITE_BUFFER_SIZE, ZSTD_COMPRESSION_LEVEL,
};

/// Configuration for opening or creating a [`crate::store::BlockStore`].
///
/// **Construction:** Use [`BlockStoreConfig::default`] and override fields, [`std::default::Default::default`]
/// with struct update syntax (`BlockStoreConfig { path: my_dir, ..Default::default() }`), or [`STR-005`](../../docs/requirements/domains/crate_structure/specs/STR-005.md) `test_config` for tiny test tunables.
///
/// **Validation:** [`crate::store::BlockStore::open`] should eventually enforce `cache_shards` is a power of two ([`TYP-008`](../../docs/requirements/domains/storage_types/specs/TYP-008.md) implementation notes); today callers should follow that invariant.
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    // --- Storage path (TYP-008) ---
    /// Root directory for the RocksDB database files (TYP-008 `path`).
    pub path: PathBuf,

    // --- In-memory blockstore caches (CAC-001 / CAC-002 precursors) ---
    /// Max blocks retained in the sharded block cache.
    pub block_cache_capacity: usize,

    /// Max headers retained in the sharded header cache.
    pub header_cache_capacity: usize,

    /// Shard count for block/header caches; must be a power of two when enforced ([`TYP-008`](../../docs/requirements/domains/storage_types/specs/TYP-008.md)).
    pub cache_shards: usize,

    /// When true, [`crate::store::BlockStore::open`] preloads recent canonical blocks ([`STR-004`](../../docs/requirements/domains/crate_structure/specs/STR-004.md), [`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub warm_cache_on_open: bool,

    /// Trailing heights (inclusive) to touch when warming, starting from tip ([`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub warm_cache_depth: u64,

    // --- RocksDB tuning (TYP-002 / TYP-003 precursors) ---
    /// RocksDB memtable / write buffer budget per column family (bytes).
    pub write_buffer_size: usize,

    /// Shared block cache for RocksDB (bytes).
    pub block_cache_size: usize,

    /// `max_open_files` passed to RocksDB options.
    pub max_open_files: i32,

    /// When true, enable BlobDB-style large-value handling for block bodies ([`TYP-003`](../../docs/requirements/domains/storage_types/specs/TYP-003.md)).
    pub enable_blob_db: bool,

    /// When true, store compressed block payloads ([`SER-001`](../../docs/requirements/domains/serialization/specs/SER-001.md)); tests often disable for simplicity ([`STR-005`](../../docs/requirements/domains/crate_structure/specs/STR-005.md)).
    pub compress_blocks: bool,

    /// Zstd level for block compression when `compress_blocks` is true.
    pub compression_level: i32,

    /// Whether to use a trained zstd dictionary ([`SER-005`](../../docs/requirements/domains/serialization/specs/SER-005.md)).
    pub use_compression_dict: bool,

    // --- Write pipeline (BLK-008 precursor) ---
    /// Max blocks batched before a pipeline flush.
    pub write_pipeline_batch_size: usize,

    /// Max wait before flushing a partial pipeline batch (milliseconds).
    pub write_pipeline_flush_ms: u64,

    /// Bounded async channel capacity feeding the write pipeline ([`BLK-008`](../../docs/requirements/domains/block_storage/specs/BLK-008.md)).
    pub write_pipeline_channel_capacity: usize,

    /// When true, sync the WAL after each write (durability vs throughput).
    pub sync_writes: bool,

    /// Hint for sequential readahead ([`BLK-006`](../../docs/requirements/domains/block_storage/specs/BLK-006.md)).
    pub readahead_size: usize,

    // --- Pruning (PRN-003 / PRN-004 precursors) ---
    /// Register compaction-time pruning when true ([`PRN-003`](../../docs/requirements/domains/pruning/specs/PRN-003_compaction_filter.md)).
    pub enable_compaction_pruning: bool,

    /// Optional floor height below which compaction may drop data; `None` disables.
    pub min_retained_height: Option<u64>,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/blockstore"),
            block_cache_capacity: DEFAULT_BLOCK_CACHE_CAPACITY,
            header_cache_capacity: DEFAULT_HEADER_CACHE_CAPACITY,
            cache_shards: 16,
            warm_cache_on_open: true,
            warm_cache_depth: 64,
            write_buffer_size: DEFAULT_WRITE_BUFFER_SIZE,
            block_cache_size: DEFAULT_BLOCK_CACHE_SIZE,
            max_open_files: DEFAULT_MAX_OPEN_FILES,
            enable_blob_db: true,
            compress_blocks: true,
            compression_level: ZSTD_COMPRESSION_LEVEL,
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
