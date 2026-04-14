//! `BlockStoreConfig` — paths, cache sizes, RocksDB tuning hooks.
//!
//! **Requirements**
//! - [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md), [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)
//! - Full field set / defaults: [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md).

/// Configuration for opening or creating a `BlockStore`.
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    /// Root directory for the RocksDB database files.
    pub db_path: std::path::PathBuf,
    /// When true, [`crate::store::BlockStore::open`] preloads recent canonical blocks into the hot path ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub warm_cache_on_open: bool,
    /// Number of trailing heights (inclusive) to touch when warming, starting from tip ([`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub warm_cache_depth: u64,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            db_path: std::path::PathBuf::from("dig_blockstore_data"),
            warm_cache_on_open: false,
            warm_cache_depth: 64,
        }
    }
}
