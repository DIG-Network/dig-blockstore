//! # STR-005 — shared test infrastructure (temp dirs, deterministic blocks, `test_config`, chains)
//!
//! **Trace**
//! - [`STR-005.md`](../../../docs/requirements/domains/crate_structure/specs/STR-005.md) — normative helpers + acceptance rows
//! - [`NORMATIVE` crate_structure](../../../docs/requirements/domains/crate_structure/NORMATIVE.md) — test config expectations
//!
//! ## What this file proves
//!
//! Each test maps to the STR-005 acceptance checklist and/or the STR-005 test-plan table. We exercise
//! helpers from [`tests/common/mod.rs`](../common/mod.rs) only (no production shortcuts) so later domain
//! tests can rely on the same contracts.

#[path = "../common/mod.rs"]
mod common;

use chia_protocol::Bytes32;

use common::{build_chain, temp_blockstore_dir, test_block, test_config, test_header};

#[test]
fn test_temp_dir_created() {
    // **Acceptance:** `temp_blockstore_dir()` creates a directory that exists on disk.
    let (_guard, path) = temp_blockstore_dir();
    assert!(
        path.is_dir(),
        "RocksDB consumers need a real directory; see STR-005 temp helper"
    );
}

#[test]
fn test_temp_dir_cleanup() {
    // **Acceptance:** dropping the `TempDir` guard deletes the directory (STR-005 / tempfile contract).
    let path = {
        let (_guard, path) = temp_blockstore_dir();
        assert!(path.exists());
        path
    };
    assert!(
        !path.exists(),
        "tempfile::TempDir should remove the tree on drop (STR-005 acceptance)"
    );
}

#[test]
fn test_block_deterministic() {
    // **Test plan:** `test_block_deterministic` — same args → bit-identical [`L2Block`] (STR-005).
    let parent = Bytes32::new([7u8; 32]);
    let a = test_block(9, parent);
    let b = test_block(9, parent);
    assert_eq!(a.hash(), b.hash());
    assert_eq!(
        bincode::serialize(&a).unwrap(),
        bincode::serialize(&b).unwrap()
    );
}

#[test]
fn test_block_height_correct() {
    // **Test plan:** `test_block_height_correct` — header height matches requested height.
    let b = test_block(42, Bytes32::default());
    assert_eq!(b.height(), 42);
    assert_eq!(b.header.height, 42);
}

#[test]
fn test_block_parent_hash_correct() {
    // **Acceptance:** `test_block()` uses the provided `parent_hash` (feeds [`L2BlockHeader::parent_hash`]).
    let parent = Bytes32::new([0xabu8; 32]);
    let b = test_block(1, parent);
    assert_eq!(b.header.parent_hash, parent);
}

#[test]
fn test_header_matches_block() {
    // **Acceptance:** `test_header()` matches the header embedded in `test_block()` for the same inputs.
    let parent = Bytes32::new([3u8; 32]);
    let h = test_header(5, parent);
    let b = test_block(5, parent);
    assert_eq!(h, b.header);
}

#[test]
fn test_config_small_caches() {
    // **Test plan / acceptance:** small in-memory cache capacities for unit tests (STR-005 example uses 10 / 20).
    let cfg = test_config(std::path::PathBuf::from("/tmp/dig_blockstore_test_only"));
    assert_eq!(cfg.block_cache_capacity, 10);
    assert_eq!(cfg.header_cache_capacity, 20);
    assert_eq!(cfg.cache_shards, 2);
    assert_eq!(cfg.warm_cache_depth, 10);
}

#[test]
fn test_config_disables_blob_and_compression() {
    // **Acceptance:** blob DB off, compression off, low zstd level — keeps store tests predictable.
    let cfg = test_config(std::path::PathBuf::from("/tmp/dig_blockstore_test_only"));
    assert!(!cfg.enable_blob_db);
    assert!(!cfg.compress_blocks);
    assert!(!cfg.use_compression_dict);
    assert_eq!(cfg.compression_level, 1);
}

#[test]
fn test_build_chain_length() {
    // **Acceptance / test plan:** `build_chain(n)` yields exactly `n` entries.
    assert_eq!(build_chain(0).len(), 0);
    assert_eq!(build_chain(5).len(), 5);
}

#[test]
fn test_build_chain_linking() {
    // **Acceptance:** each block’s `parent_hash` equals the previous block’s hash (height 0 uses default parent).
    let chain = build_chain(4);
    assert_eq!(chain[0].header.parent_hash, Bytes32::default());
    assert_eq!(chain[1].header.parent_hash, chain[0].hash());
    assert_eq!(chain[2].header.parent_hash, chain[1].hash());
    assert_eq!(chain[3].header.parent_hash, chain[2].hash());
}

#[test]
fn test_build_chain_heights() {
    // **Test plan:** heights are `0 .. n-1`.
    let chain = build_chain(6);
    for (i, b) in chain.iter().enumerate() {
        assert_eq!(b.height(), i as u64);
    }
}

#[test]
fn test_build_chain_single_is_genesis_parent() {
    // **Acceptance:** `build_chain(1)` is a single height-0 block whose parent is the default zero hash.
    let chain = build_chain(1);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].height(), 0);
    assert_eq!(chain[0].header.parent_hash, Bytes32::default());
}
