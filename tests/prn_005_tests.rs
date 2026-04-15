//! # PRN-005 — Non-canonical block pruning: delete fork blocks below pruning height
//!
//! **Trace**
//! - Spec: [`PRN-005_non_canonical_block_pruning.md`](../docs/requirements/domains/pruning/specs/PRN-005_non_canonical_block_pruning.md)
//!
//! ## What this file proves
//!
//! `prune_before_height` also removes non-canonical (fork) blocks below the target
//! height, not just canonical ones. This prevents unbounded disk growth from stored
//! fork blocks that are below the retention window.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

#[test]
fn test_non_canonical_blocks_pruned() {
    // **Proves:** PRN-005 AC §1 — non-canonical blocks below height are deleted.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Store non-canonical fork blocks at heights 2 and 3
    let fork1 = test_block(2, Bytes32::new([0xAA; 32]));
    let fork2 = test_block(3, fork1.hash());
    store.put_block(&fork1, false).expect("put fork1");
    store.put_block(&fork2, false).expect("put fork2");

    // Prune below height 5 — should remove both canonical and non-canonical blocks
    store.prune_before_height(5).expect("prune");

    // Non-canonical fork blocks below height 5 should be gone
    assert!(
        store.get_block(&fork1.hash()).expect("f1").is_none(),
        "non-canonical fork block at height 2 should be pruned"
    );
    assert!(
        store.get_block(&fork2.hash()).expect("f2").is_none(),
        "non-canonical fork block at height 3 should be pruned"
    );
}

#[test]
fn test_non_canonical_blocks_above_height_retained() {
    // **Proves:** PRN-005 AC §6 — non-canonical blocks at/above height are retained.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Store a non-canonical block at height 7
    let fork = test_block(7, Bytes32::new([0xBB; 32]));
    store.put_block(&fork, false).expect("put fork");

    // Prune below height 5 — fork at height 7 should survive
    store.prune_before_height(5).expect("prune");

    assert!(
        store.get_block(&fork.hash()).expect("f").is_some(),
        "non-canonical block at height 7 should survive pruning to 5"
    );
}

#[test]
fn test_canonical_blocks_unaffected_by_non_canonical_scan() {
    // **Proves:** PRN-005 AC §7 — canonical blocks not double-deleted.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.prune_before_height(3).expect("prune");

    // Canonical blocks at/above 3 should still exist
    for block in &chain[3..] {
        assert!(
            store.get_block(&block.hash()).expect("g").is_some(),
            "canonical block at height {} should survive",
            block.height()
        );
    }
}
