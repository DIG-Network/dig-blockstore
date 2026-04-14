//! # TYP-002 — Metadata keys (`META_*`), `SCHEMA_VERSION`, and RocksDB default tunables
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`TYP-002.md`](../docs/requirements/domains/storage_types/specs/TYP-002.md) — string keys, `SCHEMA_VERSION`, `DEFAULT_*`, `ZSTD_COMPRESSION_LEVEL`
//! - [`NORMATIVE` TYP-002](../docs/requirements/domains/storage_types/NORMATIVE.md#typ-002-metadata-keys-and-rocksdb-defaults)
//! - [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! Row-aligned with the TYP-002 test-plan table. Imports use the **crate root** ([`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md))
//! so we exercise the same path application code uses for `CF_METADATA` keys and shared tuning numbers.
//!
//! **`BlockStoreConfig::default`** is cross-checked against [`DEFAULT_BLOCK_CACHE_CAPACITY`](dig_blockstore::DEFAULT_BLOCK_CACHE_CAPACITY),
//! [`DEFAULT_WRITE_BUFFER_SIZE`](dig_blockstore::DEFAULT_WRITE_BUFFER_SIZE), etc., proving [`TYP-002` implementation notes](
//! ../docs/requirements/domains/storage_types/specs/TYP-002.md#implementation-notes) (“used by `BlockStoreConfig::default()`”) is satisfied
//! for the fields TYP-002 enumerates.

use dig_blockstore::{
    BlockStoreConfig, DEFAULT_BLOCK_CACHE_CAPACITY, DEFAULT_BLOCK_CACHE_SIZE,
    DEFAULT_BLOOM_BITS_PER_KEY, DEFAULT_HEADER_CACHE_CAPACITY, DEFAULT_MAX_OPEN_FILES,
    DEFAULT_WRITE_BUFFER_SIZE, META_GENESIS_HASH, META_MIN_HEIGHT, META_SCHEMA_VERSION, META_TIP,
    META_ZSTD_DICT, SCHEMA_VERSION, ZSTD_COMPRESSION_LEVEL,
};

#[test]
fn test_meta_tip_value() {
    assert_eq!(META_TIP, "tip");
}

#[test]
fn test_meta_genesis_hash_value() {
    assert_eq!(META_GENESIS_HASH, "genesis_hash");
}

#[test]
fn test_meta_min_height_value() {
    assert_eq!(META_MIN_HEIGHT, "min_height");
}

#[test]
fn test_meta_schema_version_value() {
    assert_eq!(META_SCHEMA_VERSION, "schema_version");
}

#[test]
fn test_meta_zstd_dict_value() {
    assert_eq!(META_ZSTD_DICT, "zstd_dict");
}

#[test]
fn test_schema_version() {
    assert_eq!(SCHEMA_VERSION, 1);
}

#[test]
fn test_write_buffer_size_default() {
    assert_eq!(DEFAULT_WRITE_BUFFER_SIZE, 64 * 1024 * 1024);
    assert_eq!(DEFAULT_WRITE_BUFFER_SIZE, 67_108_864);
}

#[test]
fn test_block_cache_size_default() {
    assert_eq!(DEFAULT_BLOCK_CACHE_SIZE, 128 * 1024 * 1024);
    assert_eq!(DEFAULT_BLOCK_CACHE_SIZE, 134_217_728);
}

#[test]
fn test_max_open_files_default() {
    assert_eq!(DEFAULT_MAX_OPEN_FILES, 1000);
}

#[test]
fn test_bloom_bits_default() {
    assert_eq!(DEFAULT_BLOOM_BITS_PER_KEY, 10);
}

#[test]
fn test_cache_capacity_defaults() {
    assert_eq!(DEFAULT_BLOCK_CACHE_CAPACITY, 1000);
    assert_eq!(DEFAULT_HEADER_CACHE_CAPACITY, 2000);
}

#[test]
fn test_compression_level_default() {
    assert_eq!(ZSTD_COMPRESSION_LEVEL, 3);
}

#[test]
fn test_block_store_config_default_uses_typ002_numeric_constants() {
    let c = BlockStoreConfig::default();
    assert_eq!(c.block_cache_capacity, DEFAULT_BLOCK_CACHE_CAPACITY);
    assert_eq!(c.header_cache_capacity, DEFAULT_HEADER_CACHE_CAPACITY);
    assert_eq!(c.write_buffer_size, DEFAULT_WRITE_BUFFER_SIZE);
    assert_eq!(c.block_cache_size, DEFAULT_BLOCK_CACHE_SIZE);
    assert_eq!(c.max_open_files, DEFAULT_MAX_OPEN_FILES);
    assert_eq!(c.compression_level, ZSTD_COMPRESSION_LEVEL);
}
