//! # CAN-005 — `extend_chain`: store block + set canonical + update tip atomically
//!
//! **Trace**
//! - Spec + test plan: [`CAN-005.md`](../docs/requirements/domains/canonical_chain/specs/CAN-005.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-005)](../docs/requirements/domains/canonical_chain/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! `extend_chain` is the primary block ingestion API for normal chain-following. It
//! combines three operations into one call: store the block, mark it canonical, and
//! advance the chain tip. These tests prove:
//!
//! 1. **New block** — a novel block returns `Ok(true)`, is retrievable, is canonical,
//!    and the tip is updated.
//! 2. **Duplicate** — re-extending with the same block returns `Ok(false)` without
//!    modifying tip or canonical state.
//! 3. **Chain building** — extending 10 linked blocks produces a correct canonical
//!    chain with tip at the highest height.
//! 4. **Tip progression** — after each `extend_chain`, `tip()` reflects the latest block.
//!
//! ## Chia analogy
//!
//! Corresponds to `Blockchain.receive_block` → `BlockStore.add_full_block` in Chia,
//! where the block is stored, the peak is updated, and the height map is advanced.
//! DIG's `extend_chain` is the storage-layer portion of that pipeline.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_blockstore::{BlockStore, ChainTip};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

#[test]
fn test_extend_chain_new_block() {
    // **Proves:** CAN-005 AC §1-§4 — new block stored, canonical, tip updated, returns true.
    //
    // **Requirement complete when:** After extend_chain with a genesis block:
    // (1) returns Ok(true), (2) get_block returns the block, (3) get_hash_by_height(0)
    // returns its hash, (4) tip() matches hash and height.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = test_block(0, Bytes32::default());
    let hash = genesis.hash();

    let result = store.extend_chain(&genesis).expect("extend_chain");
    assert!(result, "novel block must return true");

    // Block is retrievable
    let got = store.get_block(&hash).expect("get").expect("present");
    assert_eq!(got.hash(), hash);

    // Block is canonical
    let canonical_hash = store
        .get_hash_by_height(0)
        .expect("height")
        .expect("canonical");
    assert_eq!(canonical_hash, hash);

    // Tip is updated
    assert_eq!(store.tip(), Some(ChainTip { hash, height: 0 }));
}

#[test]
fn test_extend_chain_duplicate_returns_false() {
    // **Proves:** CAN-005 AC §5-§6 — duplicate block returns false, no state changes.
    //
    // **Requirement complete when:** After extending with the same block twice, the second
    // call returns Ok(false) and tip remains unchanged from the first extension.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let genesis = test_block(0, Bytes32::default());

    assert!(store.extend_chain(&genesis).expect("first"));
    let tip_after_first = store.tip();

    let second = store.extend_chain(&genesis).expect("second");
    assert!(!second, "duplicate must return false");
    assert_eq!(
        store.tip(),
        tip_after_first,
        "tip must not change on duplicate"
    );
}

#[test]
fn test_extend_chain_builds_chain() {
    // **Proves:** CAN-005 AC §7 — 10 linked blocks produce a correct canonical chain.
    //
    // **Requirement complete when:** After extending with blocks 0..9, every height has
    // the correct canonical hash, and tip is at height 9.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);

    for block in &chain {
        assert!(
            store.extend_chain(block).expect("extend"),
            "each block should be novel"
        );
    }

    // Verify canonical chain
    for (i, block) in chain.iter().enumerate() {
        let hash = store
            .get_hash_by_height(i as u64)
            .expect("h")
            .expect("canonical");
        assert_eq!(hash, block.hash(), "canonical hash at height {i}");
    }

    // Verify tip
    assert_eq!(
        store.tip(),
        Some(ChainTip {
            hash: chain[9].hash(),
            height: 9,
        })
    );
}

#[test]
fn test_extend_chain_tip_progression() {
    // **Proves:** CAN-005 AC §4 — after each extend_chain, tip() reflects the latest block.
    //
    // **Requirement complete when:** Tip advances from height 0 to 4 as each block is extended.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);

    for (i, block) in chain.iter().enumerate() {
        store.extend_chain(block).expect("extend");
        let tip = store.tip().expect("tip must exist");
        assert_eq!(tip.hash, block.hash(), "tip hash at step {i}");
        assert_eq!(tip.height, i as u64, "tip height at step {i}");
    }
}

#[test]
fn test_extend_chain_block_retrievable_by_hash() {
    // **Proves:** CAN-005 AC §2 — after extend_chain, the block is retrievable via get_block.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);

    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Verify every block is retrievable by hash
    for block in &chain {
        let got = store
            .get_block(&block.hash())
            .expect("get")
            .expect("present");
        assert_eq!(got.hash(), block.hash());
    }
}

#[test]
fn test_extend_chain_read_only_error() {
    // **Proves:** CAN-005 + STR-004 — extend_chain on a read-only store must fail.
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open rw");
        store
            .extend_chain(&test_block(0, Bytes32::default()))
            .expect("genesis");
    }
    let ro = BlockStore::open_readonly(path.as_path()).expect("ro");
    let block = test_block(1, Bytes32::new([0x11; 32]));
    assert!(
        ro.extend_chain(&block).is_err(),
        "read-only extend_chain must error"
    );
}
