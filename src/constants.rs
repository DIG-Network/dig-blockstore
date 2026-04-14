//! Column family names (`CF_*`) and metadata keys (`META_*`) for RocksDB.
//!
//! **Normative links**
//! - Column family string values: [`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)
//! - Metadata keys: [`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)
//! - Re-export contract (crate root): upcoming [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md)
//!
//! **Rationale:** CF names are stable API — changing them breaks existing databases
//! (see TYP-001 “MUST NOT be changed after initial deployment”).

/// Column family for serialized [`dig_block::L2Block`] payloads (compressed).
pub const CF_BLOCKS: &str = "blocks";

/// Column family for serialized [`dig_block::L2BlockHeader`] values.
pub const CF_HEADERS: &str = "headers";

/// Column family for [`dig_block::AttestedBlock`] records.
pub const CF_ATTESTED: &str = "attested";

/// Column family for the height → hash canonical index (cold path; see also `canonical/`).
pub const CF_CANONICAL: &str = "canonical";

/// Column family for [`StoredCheckpoint`](crate::types::StoredCheckpoint) records.
pub const CF_CHECKPOINTS: &str = "checkpoints";

/// Column family for small key-value metadata (tip, genesis hash, schema version, …).
pub const CF_METADATA: &str = "metadata";

/// Metadata key: current chain tip ([`crate::types::ChainTip`] encoding).
pub const META_TIP: &str = "tip";

/// Metadata key: genesis block hash (32 bytes).
pub const META_GENESIS_HASH: &str = "genesis_hash";

/// Metadata key: minimum retained height for pruning.
pub const META_MIN_HEIGHT: &str = "min_height";

/// Metadata key: on-disk schema version (u64 LE).
pub const META_SCHEMA_VERSION: &str = "schema_version";

/// Metadata key: trained zstd dictionary bytes ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).
pub const META_ZSTD_DICT: &str = "zstd_dict";

/// All RocksDB column families opened by [`crate::store::BlockStore::open`] (order is not significant).
pub const ALL_COLUMN_FAMILIES: &[&str] = &[
    CF_BLOCKS,
    CF_HEADERS,
    CF_ATTESTED,
    CF_CANONICAL,
    CF_CHECKPOINTS,
    CF_METADATA,
];
