//! # TYP-008 — [`dig_blockstore::BlockStoreConfig`] defaults and field surface
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-008.md`](../../docs/requirements/domains/storage_types/specs/TYP-008.md)
//! - NORMATIVE: [`NORMATIVE.md`](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-008-blockstoreconfig-struct)
//! - Verification: [`VERIFICATION.md`](../../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Defaults:** Each test maps to a row in the TYP-008 test-plan table, proving [`BlockStoreConfig::default`]
//!   matches production-oriented values ([`TYP-008` default table](../../docs/requirements/domains/storage_types/specs/TYP-008.md#field-summary)).
//!   Numeric cache / RocksDB / zstd fields are additionally aligned with [`TYP-002`](../../docs/requirements/domains/storage_types/specs/TYP-002.md)
//!   via [`dig_blockstore::constants`] inside [`src/config.rs`](../../src/config.rs).
//! - **Path:** Default directory is relative `data/blockstore` (manual [`Default`] — not `PathBuf::default()`).
//! - **Extensions:** Beyond the short TYP-008 markdown excerpt, this crate exposes `warm_cache_depth`,
//!   `write_pipeline_channel_capacity`, and `readahead_size` for [`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md),
//!   [`BLK-008`](../../docs/requirements/domains/block_storage/specs/BLK-008.md), and [`BLK-006`](../../docs/requirements/domains/block_storage/specs/BLK-006.md);
//!   defaults are asserted so future refactors do not silently change operator-visible behavior.
//! - **Override / clone:** Structural update and [`Clone`] prove the type is a mutable, copyable config bag without requiring [`dig_blockstore::BlockStore`].
//!
//! **Note:** [`STR-005`](../../docs/requirements/domains/crate_structure/specs/STR-005.md) `test_config` intentionally shrinks caches; that does not contradict TYP-008 — it overrides defaults for speed.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use dig_blockstore::{
    BlockStoreConfig, DEFAULT_BLOCK_CACHE_CAPACITY, DEFAULT_BLOCK_CACHE_SIZE,
    DEFAULT_HEADER_CACHE_CAPACITY, DEFAULT_MAX_OPEN_FILES, DEFAULT_WRITE_BUFFER_SIZE,
    ZSTD_COMPRESSION_LEVEL,
};

/// Compare two configs field-by-field ([`BlockStoreConfig`] does not derive [`PartialEq`] per TYP-008).
fn assert_all_fields_equal(a: &BlockStoreConfig, b: &BlockStoreConfig) {
    assert_eq!(a.path, b.path);
    assert_eq!(a.block_cache_capacity, b.block_cache_capacity);
    assert_eq!(a.header_cache_capacity, b.header_cache_capacity);
    assert_eq!(a.cache_shards, b.cache_shards);
    assert_eq!(a.warm_cache_on_open, b.warm_cache_on_open);
    assert_eq!(a.warm_cache_depth, b.warm_cache_depth);
    assert_eq!(a.write_buffer_size, b.write_buffer_size);
    assert_eq!(a.block_cache_size, b.block_cache_size);
    assert_eq!(a.max_open_files, b.max_open_files);
    assert_eq!(a.enable_blob_db, b.enable_blob_db);
    assert_eq!(a.compress_blocks, b.compress_blocks);
    assert_eq!(a.compression_level, b.compression_level);
    assert_eq!(a.use_compression_dict, b.use_compression_dict);
    assert_eq!(a.write_pipeline_batch_size, b.write_pipeline_batch_size);
    assert_eq!(a.write_pipeline_flush_ms, b.write_pipeline_flush_ms);
    assert_eq!(
        a.write_pipeline_channel_capacity,
        b.write_pipeline_channel_capacity
    );
    assert_eq!(a.sync_writes, b.sync_writes);
    assert_eq!(a.readahead_size, b.readahead_size);
    assert_eq!(a.enable_compaction_pruning, b.enable_compaction_pruning);
    assert_eq!(a.min_retained_height, b.min_retained_height);
}

#[test]
fn test_default_path() {
    // **Proves:** TYP-008 `test_default_path` — relative `data/blockstore` layout from spec §3.6.
    assert_eq!(
        BlockStoreConfig::default().path,
        PathBuf::from("data/blockstore")
    );
}

#[test]
fn test_default_block_cache_capacity() {
    assert_eq!(
        BlockStoreConfig::default().block_cache_capacity,
        DEFAULT_BLOCK_CACHE_CAPACITY
    );
    assert_eq!(BlockStoreConfig::default().block_cache_capacity, 1000);
}

#[test]
fn test_default_header_cache_capacity() {
    assert_eq!(
        BlockStoreConfig::default().header_cache_capacity,
        DEFAULT_HEADER_CACHE_CAPACITY
    );
    assert_eq!(BlockStoreConfig::default().header_cache_capacity, 2000);
}

#[test]
fn test_default_cache_shards() {
    assert_eq!(BlockStoreConfig::default().cache_shards, 16);
}

#[test]
fn test_default_warm_cache_on_open() {
    assert!(BlockStoreConfig::default().warm_cache_on_open);
}

#[test]
fn test_default_write_buffer_size() {
    let c = BlockStoreConfig::default();
    assert_eq!(c.write_buffer_size, DEFAULT_WRITE_BUFFER_SIZE);
    assert_eq!(c.write_buffer_size, 67_108_864);
}

#[test]
fn test_default_block_cache_size() {
    let c = BlockStoreConfig::default();
    assert_eq!(c.block_cache_size, DEFAULT_BLOCK_CACHE_SIZE);
    assert_eq!(c.block_cache_size, 134_217_728);
}

#[test]
fn test_default_max_open_files() {
    assert_eq!(
        BlockStoreConfig::default().max_open_files,
        DEFAULT_MAX_OPEN_FILES
    );
    assert_eq!(BlockStoreConfig::default().max_open_files, 1000);
}

#[test]
fn test_default_enable_blob_db() {
    assert!(BlockStoreConfig::default().enable_blob_db);
}

#[test]
fn test_default_compress_blocks() {
    assert!(BlockStoreConfig::default().compress_blocks);
}

#[test]
fn test_default_compression_level() {
    assert_eq!(
        BlockStoreConfig::default().compression_level,
        ZSTD_COMPRESSION_LEVEL
    );
    assert_eq!(BlockStoreConfig::default().compression_level, 3);
}

#[test]
fn test_default_use_compression_dict() {
    assert!(BlockStoreConfig::default().use_compression_dict);
}

#[test]
fn test_default_write_pipeline_batch_size() {
    assert_eq!(BlockStoreConfig::default().write_pipeline_batch_size, 64);
}

#[test]
fn test_default_write_pipeline_flush_ms() {
    assert_eq!(BlockStoreConfig::default().write_pipeline_flush_ms, 100);
}

#[test]
fn test_default_sync_writes() {
    assert!(!BlockStoreConfig::default().sync_writes);
}

#[test]
fn test_default_enable_compaction_pruning() {
    assert!(!BlockStoreConfig::default().enable_compaction_pruning);
}

#[test]
fn test_default_min_retained_height() {
    assert!(BlockStoreConfig::default().min_retained_height.is_none());
}

#[test]
fn test_default_extension_fields_match_crate_contract() {
    // **Proves:** Non-TYP-008-table fields still have stable defaults documented in `src/config.rs`.
    let c = BlockStoreConfig::default();
    assert_eq!(c.warm_cache_depth, 64);
    assert_eq!(c.write_pipeline_channel_capacity, 256);
    assert_eq!(c.readahead_size, 2_097_152);
}

#[test]
fn test_override_fields() {
    // **Proves:** TYP-008 `test_override_fields` — structural update leaves other defaults intact.
    let tmp = PathBuf::from("typ008_override_fixture_path");
    let c = BlockStoreConfig {
        path: tmp.clone(),
        block_cache_capacity: 50,
        header_cache_capacity: 75,
        ..Default::default()
    };
    assert_eq!(c.path, tmp);
    assert_eq!(c.block_cache_capacity, 50);
    assert_eq!(c.header_cache_capacity, 75);
    assert_eq!(c.cache_shards, 16);
    assert!(c.warm_cache_on_open);
}

#[test]
fn test_clone() {
    // **Proves:** TYP-008 `test_clone` — [`Clone`] duplicates every field for cheap config snapshots.
    let a = BlockStoreConfig::default();
    let b = a.clone();
    assert_all_fields_equal(&a, &b);
}

#[test]
fn test_debug_format() {
    // **Proves:** [`Debug`] is usable in logs (no panic); substring guard catches accidental field renames.
    let s = format!("{:?}", BlockStoreConfig::default());
    assert!(
        s.contains("path") && s.contains("block_cache_capacity"),
        "unexpected Debug: {s}"
    );
}
