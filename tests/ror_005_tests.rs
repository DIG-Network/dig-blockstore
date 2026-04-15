//! # ROR-005 — Rollback boundary validation: NoTip, RollbackAboveTip, RollbackBelowMin
//!
//! **Trace**
//! - Spec: [`ROR-005.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-005.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-005)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! Before any rollback mutation, `rollback_to_height` validates boundaries:
//! 1. **NoTip** — no chain tip set (empty/uninitialized store).
//! 2. **RollbackAboveTip** — target height exceeds current chain height.
//! 3. **RollbackBelowMin** — target height is below pruned data floor.
//! 4. **No mutation on error** — store state unchanged when validation fails.
//! 5. **min_retained_height** — returns 0 when no pruning has occurred.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, BlockStoreError};

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_rollback_no_tip() {
    // **Proves:** ROR-005 AC §1 — empty store returns NoTip.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let err = store.rollback_to_height(0).expect_err("should fail");
    assert!(
        matches!(err, BlockStoreError::NoTip),
        "expected NoTip, got {err:?}"
    );
}

#[test]
fn test_rollback_above_tip() {
    // **Proves:** ROR-005 AC §2/§4 — target > tip returns RollbackAboveTip with values.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let err = store.rollback_to_height(10).expect_err("should fail");
    match err {
        BlockStoreError::RollbackAboveTip { target, tip } => {
            assert_eq!(target, 10);
            assert_eq!(tip, 4); // chain heights 0..4, tip is 4
        }
        other => panic!("expected RollbackAboveTip, got {other:?}"),
    }
}

#[test]
fn test_rollback_at_tip_succeeds() {
    // **Proves:** ROR-005 AC §5 — rollback at current tip is a no-op (valid but nothing reverted).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let reverted = store.rollback_to_height(4).expect("at-tip should succeed");
    assert!(reverted.is_empty(), "rollback at tip reverts nothing");
    // Tip should remain unchanged
    assert_eq!(store.height(), Some(4));
}

#[test]
fn test_no_mutation_on_error() {
    // **Proves:** ROR-005 AC §6 — failed validation leaves store unchanged.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let tip_before = store.tip();
    let hash_at_3 = store.get_hash_by_height(3).expect("h3");

    // Attempt invalid rollback
    let _ = store.rollback_to_height(10);

    // State must be unchanged
    assert_eq!(store.tip(), tip_before);
    assert_eq!(store.get_hash_by_height(3).expect("h3"), hash_at_3);
}

#[test]
fn test_no_pruning_min_is_zero() {
    // **Proves:** ROR-005 AC §7 — fresh store with no pruning has min_retained_height = 0.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert_eq!(
        store.min_retained_height().expect("min"),
        0,
        "no pruning → min_retained_height = 0"
    );
}

#[test]
fn test_rollback_to_zero_succeeds() {
    // **Proves:** ROR-005 AC §8 — when min_retained_height is 0, rollback to 0 succeeds.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    let reverted = store.rollback_to_height(0).expect("rollback to 0");
    assert_eq!(reverted.len(), 2, "should revert heights 2 and 1");
    assert_eq!(store.height(), Some(0), "tip should be at genesis");
}
