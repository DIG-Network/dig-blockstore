//! # PRN-001 — `prune_before_height`: remove blocks/headers/attestations/canonical below height
//!
//! **Trace**
//! - Spec: [`PRN-001_prune_before_height.md`](../docs/requirements/domains/pruning/specs/PRN-001_prune_before_height.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_prune_removes_canonical_entries() {
    // **Proves:** PRN-001 AC §1 — CF_CANONICAL cleaned for pruned heights.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.prune_before_height(5).expect("prune");

    // Heights 0..4 should no longer have canonical entries
    for h in 0..5 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_none(),
            "height {h} should be pruned from canonical index"
        );
    }
    // Heights 5..9 should still be canonical
    for h in 5..10 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_some(),
            "height {h} should survive pruning"
        );
    }
}

#[test]
fn test_prune_removes_block_data() {
    // **Proves:** PRN-001 AC §1 — CF_BLOCKS cleaned for pruned blocks.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.prune_before_height(4).expect("prune");

    // Pruned blocks should not be retrievable
    for block in &chain[..4] {
        assert!(
            store.get_block(&block.hash()).expect("get").is_none(),
            "pruned block should be gone"
        );
    }
    // Retained blocks should be fine
    for block in &chain[4..] {
        assert!(
            store.get_block(&block.hash()).expect("get").is_some(),
            "retained block should exist"
        );
    }
}

#[test]
fn test_prune_returns_count() {
    // **Proves:** PRN-001 AC §5 — returns accurate count.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let count = store.prune_before_height(5).expect("prune");
    assert!(
        count >= 5,
        "should prune at least 5 canonical blocks, got {count}"
    );
}

#[test]
fn test_prune_noop_below_min() {
    // **Proves:** PRN-001 AC §6 — no-op when height <= min_retained_height.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    store.prune_before_height(5).expect("first prune");
    let count = store.prune_before_height(3).expect("second prune");
    assert_eq!(count, 0, "below current min = no-op");
}

#[test]
fn test_prune_updates_min_retained() {
    // **Proves:** PRN-001 AC §4 — min_retained_height updated.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    store.prune_before_height(7).expect("prune");
    assert_eq!(store.min_retained_height().expect("min"), 7);
}
