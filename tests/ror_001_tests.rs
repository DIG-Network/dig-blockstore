//! # ROR-001 — `rollback_to_height`: revert canonical chain without deleting blocks
//!
//! **Trace**
//! - Spec: [`ROR-001.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-001.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-001)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! `rollback_to_height(target)` reverts the canonical index from `tip.height` down to
//! `target + 1`, truncates canonical.bin, updates the tip, and marks reverted blocks as
//! non-canonical in the record cache. Block data in CF_BLOCKS/CF_HEADERS is preserved
//! (fork preservation per ROR-004). Returns reverted hashes in descending order.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_rollback_removes_canonical_entries() {
    // **Proves:** ROR-001 AC — CF_CANONICAL entries above target are removed.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let reverted = store.rollback_to_height(5).expect("rollback");

    // Heights 6..9 should no longer be canonical
    for h in 6..10 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_none(),
            "height {h} should not be canonical after rollback to 5"
        );
    }
    // Heights 0..5 should remain canonical
    for h in 0..=5 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_some(),
            "height {h} should remain canonical"
        );
    }
    // Reverted in descending order
    assert_eq!(reverted.len(), 4, "heights 9,8,7,6 reverted");
    assert_eq!(reverted[0], chain[9].hash(), "tip first");
    assert_eq!(reverted[3], chain[6].hash(), "lowest reverted last");
}

#[test]
fn test_rollback_updates_tip() {
    // **Proves:** ROR-001 AC — tip is updated to block at target_height.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.rollback_to_height(3).expect("rollback");

    let tip = store.tip().expect("tip");
    assert_eq!(tip.height, 3);
    assert_eq!(tip.hash, chain[3].hash());
}

#[test]
fn test_rollback_preserves_block_data() {
    // **Proves:** ROR-001 + ROR-004 — block data in CF_BLOCKS survives rollback.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(6);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.rollback_to_height(2).expect("rollback");

    // ALL 6 blocks should still be retrievable by hash
    for (i, block) in chain.iter().enumerate() {
        let got = store
            .get_block(&block.hash())
            .expect("get")
            .expect("present");
        assert_eq!(got.hash(), block.hash(), "block {i} preserved");
    }
}

#[test]
fn test_rollback_marks_records_non_canonical() {
    // **Proves:** ROR-001 AC — reverted blocks have in_canonical_chain=false in record cache.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.rollback_to_height(2).expect("rollback");

    // Heights 3,4 should have in_canonical_chain=false
    for block in &chain[3..] {
        let rec = store.get_record(&block.hash()).expect("rec").expect("some");
        assert!(
            !rec.in_canonical_chain,
            "block at height {} should be non-canonical after rollback",
            rec.height
        );
    }
    // Heights 0..2 should still be canonical
    for block in &chain[..=2] {
        let rec = store.get_record(&block.hash()).expect("rec").expect("some");
        assert!(
            rec.in_canonical_chain,
            "block at height {} should still be canonical",
            rec.height
        );
    }
}

#[test]
fn test_rollback_at_tip_is_noop() {
    // **Proves:** ROR-001 — rollback at current tip returns empty vec and changes nothing.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let tip_before = store.tip();
    let reverted = store.rollback_to_height(4).expect("at-tip rollback");
    assert!(reverted.is_empty());
    assert_eq!(store.tip(), tip_before);
}

#[test]
fn test_rollback_returns_descending_order() {
    // **Proves:** ROR-001 AC — hashes in descending height order (tip first).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let reverted = store.rollback_to_height(3).expect("rollback");
    assert_eq!(reverted.len(), 4);
    // Descending: 7, 6, 5, 4
    for (i, hash) in reverted.iter().enumerate() {
        let expected_height = 7 - i;
        assert_eq!(
            *hash,
            chain[expected_height].hash(),
            "index {i} = height {expected_height}"
        );
    }
}
