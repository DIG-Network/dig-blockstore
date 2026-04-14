//! Column family names (`CF_*`) and metadata keys (`META_*`) for RocksDB.
//!
//! **Normative links**
//! - Column family string values: [`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)
//! - Metadata keys, `SCHEMA_VERSION`, RocksDB / cache numeric defaults: [`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)
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

/// Metadata key: current chain tip — value is [`crate::types::ChainTip`] **40-byte** encoding ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md), [`TYP-006`](../docs/requirements/domains/storage_types/specs/TYP-006.md)).
pub const META_TIP: &str = "tip";

/// Metadata key: genesis block hash — value is **32** raw bytes ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
pub const META_GENESIS_HASH: &str = "genesis_hash";

/// Metadata key: minimum retained block height for pruning — value is **8** bytes **little-endian** [`u64`] ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md), [`PRN-004`](../docs/requirements/domains/pruning/specs/PRN-004_min_retained_height_tracking.md) precursor).
pub const META_MIN_HEIGHT: &str = "min_height";

/// Metadata key: on-disk schema version — value is **8** bytes **little-endian** [`u64`], compared to [`SCHEMA_VERSION`] on open ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
pub const META_SCHEMA_VERSION: &str = "schema_version";

/// Metadata key: trained zstd dictionary bytes ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md); [`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
pub const META_ZSTD_DICT: &str = "zstd_dict";

/// On-disk schema version written under [`META_SCHEMA_VERSION`] ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md) / NORMATIVE).
///
/// **Rationale:** Bump when metadata or CF layout changes; opening older DBs without migration yields [`crate::error::BlockStoreError::SchemaMismatch`] (future wiring).
pub const SCHEMA_VERSION: u64 = 1;

// --- RocksDB & in-memory cache defaults ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)) ---

/// Default RocksDB memtable / write buffer size per CF (**64 MiB** = `67_108_864` bytes).
///
/// **Used by:** [`crate::config::BlockStoreConfig::default`](crate::config::BlockStoreConfig) ([`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)); Rocks option wiring in [`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md).
pub const DEFAULT_WRITE_BUFFER_SIZE: usize = 67_108_864;

/// Default shared RocksDB block cache size (**128 MiB** = `134_217_728` bytes).
pub const DEFAULT_BLOCK_CACHE_SIZE: usize = 134_217_728;

/// Default RocksDB [`Options::set_max_open_files`](https://docs.rs/rocksdb/latest/rocksdb/struct.Options.html) budget.
pub const DEFAULT_MAX_OPEN_FILES: i32 = 1000;

/// Default bloom-filter bits per key for CFs that use bloom ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md) applies per family).
pub const DEFAULT_BLOOM_BITS_PER_KEY: i32 = 10;

/// Default max entries in the **in-memory** block cache ([`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001.md) precursor; [`BlockStoreConfig`](crate::config::BlockStoreConfig) `block_cache_capacity`).
pub const DEFAULT_BLOCK_CACHE_CAPACITY: usize = 1000;

/// Default max entries in the **in-memory** header cache ([`CAC-002`](../docs/requirements/domains/caching/specs/CAC-002.md) precursor).
pub const DEFAULT_HEADER_CACHE_CAPACITY: usize = 2000;

/// Default zstd level for block body compression ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md); [`crate::store::BlockStore::serialize_block`] / [`crate::config::BlockStoreConfig::default`]).
pub const ZSTD_COMPRESSION_LEVEL: i32 = 3;

/// Upper bound on **decompressed** block payload size accepted by [`crate::store::BlockStore::deserialize_block`]
/// ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) — mitigates malicious zstd bombs).
///
/// **Rationale:** Bincode `L2Block` for DIG L2 is expected to stay well below this; the cap bounds allocator
/// work in [`zstd::bulk::Decompressor::decompress`].
pub const DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES: usize = 128 * 1024 * 1024;

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
