//! Column family names (`CF_*`) and metadata keys (`META_*`) for RocksDB.
//!
//! **Normative links**
//! - Column family string values: [`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)
//! - Metadata keys: [`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)
//! - Re-export contract (crate root): [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md)
//!
//! **Rationale:** CF names are stable API — changing them breaks existing RocksDB layouts
//! ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md): “MUST NOT be changed after initial deployment”).

/// Column family for serialized [`dig_block::L2Block`] data ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
///
/// **Keys:** block hash (32 bytes). **Values:** compressed serialized [`dig_block::L2Block`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
pub const CF_BLOCKS: &str = "blocks";

/// Column family for serialized [`dig_block::L2BlockHeader`] data ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
///
/// **Keys:** block hash (32 bytes). **Values:** bincode-serialized header ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
pub const CF_HEADERS: &str = "headers";

/// Column family for serialized [`dig_block::AttestedBlock`] data ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
///
/// **Keys:** block hash (32 bytes). **Values:** serialized attested block ([`BLK-009`](../docs/requirements/domains/block_storage/specs/BLK-009.md) precursor).
pub const CF_ATTESTED: &str = "attested";

/// Column family for the canonical chain index ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md); mmap hot path in [`crate::canonical`]).
///
/// **Keys:** height (8 bytes big-endian). **Values:** block hash (32 bytes).
pub const CF_CANONICAL: &str = "canonical";

/// Column family for serialized [`StoredCheckpoint`](crate::types::StoredCheckpoint) data ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
///
/// **Keys:** epoch (8 bytes big-endian). **Values:** serialized checkpoint ([`CKP-001`](../docs/requirements/domains/checkpoint_storage/specs/CKP-001.md) precursor).
pub const CF_CHECKPOINTS: &str = "checkpoints";

/// Column family for key-value metadata ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md); `META_*` keys below / [`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
///
/// **Keys:** UTF-8 string. **Values:** per-key encoding (tip bytes, genesis hash, …).
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

/// All column families opened by [`crate::store::BlockStore::open`] / read-only open ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
///
/// **Contract:** Exactly the six [`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md) names; slice order is not significant for correctness.
pub const ALL_COLUMN_FAMILIES: &[&str] = &[
    CF_BLOCKS,
    CF_HEADERS,
    CF_ATTESTED,
    CF_CANONICAL,
    CF_CHECKPOINTS,
    CF_METADATA,
];
