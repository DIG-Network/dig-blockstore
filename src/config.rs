//! `BlockStoreConfig` — paths, cache sizes, RocksDB tuning hooks.
//!
//! **Requirements**
//! - Struct home: [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md)
//! - Field completeness / defaults: [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md) (later phase).
//!
//! **Usage (future):** Constructed by node startup code and passed to [`crate::store::BlockStore::open`](crate::store::BlockStore).

/// Configuration for opening or creating a `BlockStore` ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    /// Root directory for the RocksDB database files.
    pub db_path: std::path::PathBuf,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            db_path: std::path::PathBuf::from("dig_blockstore_data"),
        }
    }
}
