//! Shared test helpers for integration tests across requirement domains.
//!
//! **Requirement:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) —
//! temporary RocksDB directories, deterministic [`dig_block::L2Block`] fixtures, small
//! [`dig_blockstore::BlockStoreConfig`], and linear fake chains.
//!
//! ## How integration tests include this module
//!
//! Rust treats each `[[test]]` binary as a separate crate root. Submodules are resolved relative to the
//! test file, so flat `tests/<prefix>_<req#>_tests.rs` crates pull this tree in with:
//! `#[path = "common/mod.rs"] mod common;`
//!
//! **Rationale:** Avoid duplicating genesis/block builders in every domain’s test file; keep
//! determinism explicit (same inputs → same [`L2Block::hash`]) so storage and canonical tests share one
//! definition of “a fake block”.

use std::path::PathBuf;

use chia_protocol::Bytes32;
use dig_block::constants::{EMPTY_ROOT, ZERO_HASH};
use dig_block::{L2Block, L2BlockHeader, Signature};
use dig_blockstore::BlockStoreConfig;
use tempfile::TempDir;

/// Creates a temporary directory for a RocksDB-backed [`dig_blockstore::BlockStore`] that is deleted when
/// the returned [`TempDir`] guard is dropped.
///
/// **Proof:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) “Temporary
/// RocksDB Directory Helper”; cleanup is [`tempfile`](https://docs.rs/tempfile/)’s `Drop` on [`TempDir`].
///
/// # Returns
///
/// `(guard, path)` — keep the guard alive for the lifetime of the store; assign `path` to
/// [`BlockStoreConfig`](dig_blockstore::BlockStoreConfig) `path` when calling [`dig_blockstore::BlockStore::open`].
pub fn temp_blockstore_dir() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempfile::tempdir should succeed in tests");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Builds a deterministic [`L2BlockHeader`] from `height`, `parent_hash`, and fixed sentinel roots.
///
/// **Spec:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) “Test Block
/// Helper” — timestamp scales with height (`height * 10`); other fields use stable protocol constants
/// ([`EMPTY_ROOT`](dig_block::constants::EMPTY_ROOT), [`ZERO_HASH`](dig_block::constants::ZERO_HASH)) so
/// the header hash is a pure function of `(height, parent_hash)` for test purposes.
///
/// **Upstream type:** [`L2BlockHeader::new`](dig_block::L2BlockHeader::new) (DIG [`dig-block`](https://github.com/DIG-Network/dig-block) / BLK-002).
pub fn test_header(height: u64, parent_hash: Bytes32) -> L2BlockHeader {
    let l1_height = height.min(u64::from(u32::MAX)) as u32;
    let mut header = L2BlockHeader::new(
        height,
        height,
        parent_hash,
        EMPTY_ROOT,
        EMPTY_ROOT,
        EMPTY_ROOT,
        EMPTY_ROOT,
        EMPTY_ROOT,
        l1_height,
        ZERO_HASH,
        0,
        0,
        0u64,
        0u64,
        0,
        0,
        0,
        ZERO_HASH,
    );
    header.timestamp = height.saturating_mul(10);
    header
}

/// Wraps [`test_header`] in an [`L2Block`] with an empty body and default proposer signature.
///
/// **Identity:** [`L2Block::hash`](dig_block::L2Block::hash) delegates to the header; empty spend bundles
/// keep the Merkle roots in the header consistent with the zeroed counts from [`L2BlockHeader::new`].
pub fn test_block(height: u64, parent_hash: Bytes32) -> L2Block {
    let header = test_header(height, parent_hash);
    L2Block::new(header, vec![], vec![], Signature::default())
}

/// [`BlockStoreConfig`](dig_blockstore::BlockStoreConfig) tuned for fast, isolated unit tests.
///
/// **Normative:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) — small
/// in-memory cache capacities, reduced RocksDB budgets, **disabled block compression** (`compress_blocks: false`)
/// for cheap genesis / round-trip tests; BlobDB stays **`true`** (aligned with
/// [`BlockStoreConfig::default`]) so [`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)
/// CF options match [`dig_blockstore::BlockStore::open_readonly`]’s default-derived descriptors.
///
/// **Related:** Production defaults remain [`BlockStoreConfig::default`](dig_blockstore::BlockStoreConfig::default)
/// ([`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)).
pub fn test_config(db_path: PathBuf) -> BlockStoreConfig {
    BlockStoreConfig {
        path: db_path,
        block_cache_capacity: 10,
        header_cache_capacity: 20,
        cache_shards: 2,
        warm_cache_on_open: false,
        warm_cache_depth: 10,
        write_buffer_size: 4 * 1024 * 1024,
        block_cache_size: 8 * 1024 * 1024,
        max_open_files: 100,
        enable_blob_db: true,
        compress_blocks: false,
        compression_level: 1,
        use_compression_dict: false,
        write_pipeline_batch_size: 4,
        write_pipeline_flush_ms: 10,
        write_pipeline_channel_capacity: 8,
        sync_writes: false,
        readahead_size: 64 * 1024,
        enable_compaction_pruning: false,
        min_retained_height: None,
    }
}

/// Builds `n` blocks `[genesis … block_{n-1}]` where each block’s parent hash is the previous block’s
/// [`L2Block::hash`].
///
/// **Algorithm:** [`STR-005`](../docs/requirements/domains/crate_structure/specs/STR-005.md) “Chain
/// Builder” — seed parent with [`Bytes32::default`] (all-zero hash / genesis parent sentinel), then link.
pub fn build_chain(n: usize) -> Vec<L2Block> {
    let genesis_parent = Bytes32::default();
    let mut blocks = Vec::with_capacity(n);
    let mut parent = genesis_parent;
    for height in 0..n {
        let block = test_block(height as u64, parent);
        parent = block.hash();
        blocks.push(block);
    }
    blocks
}
