//! RocksDB **per–column-family** options ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).
//!
//! ## Contract
//!
//! - [`BlockStore::open`](crate::store::BlockStore::open) builds [`ColumnFamilyDescriptor`]s via
//!   [`column_family_descriptors`] so every [`crate::constants::ALL_COLUMN_FAMILIES`] family receives
//!   the compaction / bloom / compression / BlobDB tuning spelled out in the spec.
//! - [`BlockStore::open_readonly`](crate::store::BlockStore::open_readonly) must pass **matching**
//!   descriptors for an existing DB; we reuse the same builders with
//!   [`BlockStoreConfig::default`](crate::config::BlockStoreConfig::default) plus the caller’s
//!   `path` ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md), [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)), so
//!   STR-005 `tests/common/mod.rs` `test_config` **keeps `enable_blob_db` aligned** with
//!   [`BlockStoreConfig::default`] when tests call [`BlockStore::open_readonly`](crate::store::BlockStore::open_readonly)
//!   on the same directory (RocksDB validates CF options on reopen).
//!
//! ## Bloom filters and the `rocksdb` 0.22 API
//!
//! TYP-003’s normative snippets call `Options::set_bloom_filter`. In **rust-rocksdb 0.22**, bloom is
//! configured on [`rocksdb::BlockBasedOptions`] and attached with
//! [`rocksdb::Options::set_block_based_table_factory`]. We use **full** bloom (`block_based = false`
//! in the spec’s `set_bloom_filter(bits, false)` sense) via [`BlockBasedOptions::set_bloom_filter`].
//! Families that **must not** use bloom ([`crate::constants::CF_BLOCKS`], [`crate::constants::CF_CANONICAL`])
//! leave the default block-based table factory untouched (no `filter_policy`), which matches
//! `filter_policy=nullptr` in on-disk `OPTIONS-*` dumps.
//!
//! **Semantic links:** NORMATIVE §TYP-003 — `docs/requirements/domains/storage_types/NORMATIVE.md`

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rocksdb::{
    compaction_filter::Decision, BlockBasedOptions, ColumnFamilyDescriptor, DBCompactionStyle,
    DBCompressionType, Options,
};

use crate::config::BlockStoreConfig;
use crate::constants::{
    ALL_COLUMN_FAMILIES, CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS,
    CF_METADATA, DEFAULT_BLOOM_BITS_PER_KEY,
};

/// Minimum value size (bytes) routed to BlobDB for [`CF_BLOCKS`] when `config.enable_blob_db` ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).
pub const BLOCKS_BLOB_MIN_SIZE: u64 = 512;

/// `target_file_size_base` for [`CF_CHECKPOINTS`]: 256 MiB ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).
pub const CHECKPOINTS_TARGET_FILE_SIZE_BASE: u64 = 256 * 1024 * 1024;

/// Builds [`ColumnFamilyDescriptor`]s in **exact** [`ALL_COLUMN_FAMILIES`] order for `DB::open_cf_descriptors*`.
///
/// **Usage:** [`crate::store::BlockStore::open`], [`crate::store::BlockStore::open_readonly`].
/// **Invariant:** One descriptor per non-`default` CF name the store manages; the `rocksdb` crate
/// may still inject a `[default]` CF internally—see its `open_cf_descriptors_internal` implementation.
/// Build CF descriptors, optionally with compaction filter for pruning ([`PRN-003`]).
///
/// When `prune_threshold` is `Some`, a compaction filter is registered on CF_HEADERS
/// that drops entries where the deserialized header height is below the threshold.
/// The threshold is read from the shared `AtomicU64` with `Acquire` ordering.
pub fn column_family_descriptors(
    config: &BlockStoreConfig,
    prune_threshold: Option<Arc<AtomicU64>>,
) -> Vec<ColumnFamilyDescriptor> {
    ALL_COLUMN_FAMILIES
        .iter()
        .map(|&name| {
            let opts = match name {
                CF_BLOCKS => blocks_cf_options(config),
                CF_HEADERS => headers_cf_options(prune_threshold.clone()),
                CF_ATTESTED => attested_cf_options(),
                CF_CANONICAL => canonical_cf_options(),
                CF_CHECKPOINTS => checkpoints_cf_options(),
                CF_METADATA => metadata_cf_options(),
                _ => unreachable!("ALL_COLUMN_FAMILIES drifted from TYP-001 names: {name}"),
            };
            ColumnFamilyDescriptor::new(name, opts)
        })
        .collect()
}

/// [`CF_BLOCKS`]: Universal compaction; optional BlobDB; **no** explicit bloom ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).
pub fn blocks_cf_options(config: &BlockStoreConfig) -> Options {
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Universal);
    if config.enable_blob_db {
        opts.set_enable_blob_files(true);
        opts.set_min_blob_size(BLOCKS_BLOB_MIN_SIZE);
        opts.set_blob_compression_type(DBCompressionType::Zstd);
    }
    opts
}

/// [`CF_HEADERS`]: Level compaction; bloom ([`DEFAULT_BLOOM_BITS_PER_KEY`]); compression off.
/// [`CF_HEADERS`]: Level compaction; bloom filter; no compression; optional compaction filter ([`PRN-003`]).
///
/// When `prune_threshold` is `Some`, registers a compaction filter that deserializes
/// the header value (bincode) to extract the block height. If `height < threshold`,
/// the entry is removed during compaction. This is the secondary cleanup mechanism;
/// PRN-001 (`prune_before_height`) is the primary.
pub fn headers_cf_options(prune_threshold: Option<Arc<AtomicU64>>) -> Options {
    let mut block = BlockBasedOptions::default();
    block.set_bloom_filter(f64::from(DEFAULT_BLOOM_BITS_PER_KEY), false);
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts.set_block_based_table_factory(&block);
    opts.set_compression_type(DBCompressionType::None);
    if let Some(threshold) = prune_threshold {
        opts.set_compaction_filter("prn003_header_height_filter", move |_level, _key, value| {
            let min_height = threshold.load(Ordering::Acquire);
            if min_height == 0 {
                return Decision::Keep;
            }
            // Attempt to deserialize the header value to extract height.
            // On deserialization failure, keep the entry (don't drop potentially valid data).
            match bincode::deserialize::<dig_block::L2BlockHeader>(value) {
                Ok(header) => {
                    if header.height < min_height {
                        Decision::Remove
                    } else {
                        Decision::Keep
                    }
                }
                Err(_) => Decision::Keep,
            }
        });
    }
    opts
}

/// [`CF_ATTESTED`]: Level compaction + bloom; default compression (left at RocksDB default).
pub fn attested_cf_options() -> Options {
    let mut block = BlockBasedOptions::default();
    block.set_bloom_filter(f64::from(DEFAULT_BLOOM_BITS_PER_KEY), false);
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts.set_block_based_table_factory(&block);
    opts
}

/// [`CF_CANONICAL`]: Level compaction; **no** bloom; compression off (32-byte hashes).
pub fn canonical_cf_options() -> Options {
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts.set_compression_type(DBCompressionType::None);
    opts
}

/// [`CF_CHECKPOINTS`]: Level compaction; large SST target to reduce compaction churn.
pub fn checkpoints_cf_options() -> Options {
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts.set_target_file_size_base(CHECKPOINTS_TARGET_FILE_SIZE_BASE);
    opts
}

/// [`CF_METADATA`]: Level compaction; otherwise RocksDB defaults.
pub fn metadata_cf_options() -> Options {
    let mut opts = Options::default();
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_family_descriptors_matches_all_families_count() {
        let cfg = BlockStoreConfig::default();
        assert_eq!(
            column_family_descriptors(&cfg, None).len(),
            ALL_COLUMN_FAMILIES.len()
        );
    }
}
