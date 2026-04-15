//! # ROR-006 — `blocks_to_revert`: read-only rollback preview
//!
//! **Trace**
//! - Spec: [`ROR-006.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-006.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-006)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! `blocks_to_revert(target_height)` returns the canonical block hashes that WOULD be
//! reverted, in descending height order, WITHOUT modifying any state. This is the
//! read-only counterpart to `rollback_to_height`.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_blocks_to_revert_basic() {
    // **Proves:** ROR-006 AC §1/§2 — returns hashes for (target, tip] in descending order.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let reverted = store.blocks_to_revert(5).expect("preview");
    assert_eq!(reverted.len(), 4, "should include heights 9,8,7,6");
    // Descending order: tip first
    assert_eq!(reverted[0], chain[9].hash(), "tip hash first");
    assert_eq!(reverted[1], chain[8].hash());
    assert_eq!(reverted[2], chain[7].hash());
    assert_eq!(reverted[3], chain[6].hash(), "height 6 last");
}

#[test]
fn test_blocks_to_revert_no_tip() {
    // **Proves:** ROR-006 AC §3 — empty store returns empty vec (no error).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let reverted = store.blocks_to_revert(0).expect("preview");
    assert!(reverted.is_empty());
}

#[test]
fn test_blocks_to_revert_at_tip() {
    // **Proves:** ROR-006 AC §4 — target_height >= tip returns empty vec.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    assert!(store.blocks_to_revert(4).expect("at tip").is_empty());
    assert!(store.blocks_to_revert(10).expect("above tip").is_empty());
}

#[test]
fn test_blocks_to_revert_to_zero() {
    // **Proves:** ROR-006 — revert preview to height 0 returns all non-genesis hashes.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let reverted = store.blocks_to_revert(0).expect("to zero");
    assert_eq!(reverted.len(), 4, "heights 4,3,2,1 reverted (not 0)");
    assert_eq!(reverted[0], chain[4].hash(), "tip first");
    assert_eq!(reverted[3], chain[1].hash(), "height 1 last");
}

#[test]
fn test_blocks_to_revert_no_mutation() {
    // **Proves:** ROR-006 AC §5 — read-only, no state changes.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let tip_before = store.tip();
    let _ = store.blocks_to_revert(2).expect("preview");
    assert_eq!(store.tip(), tip_before, "tip unchanged");
    // Canonical index unchanged
    for (i, block) in chain.iter().enumerate() {
        let hash = store
            .get_hash_by_height(i as u64)
            .expect("h")
            .expect("still canonical");
        assert_eq!(hash, block.hash());
    }
}
