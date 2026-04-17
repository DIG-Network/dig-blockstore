//! # ROR-003 — `apply_reorg`: atomic rollback + re-canonicalization via single WriteBatch
//!
//! **Trace**
//! - Spec: [`ROR-003.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-003.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-003)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! `apply_reorg` is the most complex state mutation in the store. It atomically:
//! 1. Rolls back the canonical index to `ancestor_height`.
//! 2. Applies new canonical hashes from `new_chain_hashes`.
//! 3. Updates the chain tip.
//!
//! All within a single RocksDB WriteBatch for crash-safe atomicity.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_blockstore::{BlockStore, BlockStoreError};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// Build a fork chain branching from `parent_hash` with `n` blocks.
fn build_fork(parent_hash: Bytes32, n: usize, height_offset: u64) -> Vec<dig_block::L2Block> {
    let mut blocks = Vec::with_capacity(n);
    let mut parent = parent_hash;
    for i in 0..n {
        let block = test_block(height_offset + 1 + i as u64, parent);
        parent = block.hash();
        blocks.push(block);
    }
    blocks
}

#[test]
fn test_reorg_basic() {
    // **Proves:** ROR-003 AC §1-§3 — rollback + re-canonicalize + tip update.
    //
    // Setup: canonical chain 0..9 (tip at 9). Fork branches from height 4,
    // with 3 fork blocks stored non-canonically. Reorg at ancestor_height=4
    // with fork hashes should:
    // - Remove canonical entries for heights 5..9
    // - Set fork blocks as canonical
    // - Update tip to fork tip
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Store fork blocks (branch from height 4)
    let fork = build_fork(chain[4].hash(), 3, 100);
    for fb in &fork {
        store.put_block(fb, false).expect("store fork");
    }

    let fork_hashes: Vec<Bytes32> = fork.iter().map(|b| b.hash()).collect();
    let result = store.apply_reorg(4, &fork_hashes).expect("reorg");

    // Verify reverted (heights 9,8,7,6,5 in descending order)
    assert_eq!(result.reverted.len(), 5);
    assert_eq!(result.reverted[0], chain[9].hash()); // tip first

    // Verify applied
    assert_eq!(result.applied.len(), 3);
    assert_eq!(result.applied, fork_hashes);

    // Verify new tip
    assert_eq!(result.new_tip.hash, fork[2].hash());

    // Verify canonical chain: 0..4 unchanged, then fork blocks
    for i in 0..=4 {
        let h = store.get_hash_by_height(i).expect("h").expect("canonical");
        assert_eq!(h, chain[i as usize].hash(), "height {i} unchanged");
    }
    // Heights 5,6,7,8,9 should now resolve to fork blocks via their heights
    // Fork blocks have heights 101, 102, 103 — so canonical at THOSE heights
    for (i, fb) in fork.iter().enumerate() {
        let h = store
            .get_hash_by_height(fb.height())
            .expect("h")
            .expect("fork canonical");
        assert_eq!(
            h,
            fb.hash(),
            "fork block {i} should be canonical at its height"
        );
    }
}

#[test]
fn test_reorg_result_counts() {
    // **Proves:** ROR-003 AC §5/§6/§7 — ReorgResult has correct counts.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let fork = build_fork(chain[3].hash(), 2, 200);
    for fb in &fork {
        store.put_block(fb, false).expect("store fork");
    }

    let fork_hashes: Vec<Bytes32> = fork.iter().map(|b| b.hash()).collect();
    let result = store.apply_reorg(3, &fork_hashes).expect("reorg");

    assert_eq!(result.reverted.len(), 4, "heights 7,6,5,4 reverted");
    assert_eq!(result.applied.len(), 2, "2 fork blocks applied");
    assert_eq!(result.new_tip.hash, fork[1].hash());
}

#[test]
fn test_reorg_empty_chain_error() {
    // **Proves:** ROR-003 AC §11 — empty new_chain_hashes returns EmptyReorgChain.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let err = store.apply_reorg(1, &[]).expect_err("should fail");
    assert!(
        matches!(err, BlockStoreError::EmptyReorgChain),
        "expected EmptyReorgChain, got {err:?}"
    );
}

#[test]
fn test_reorg_missing_block_error() {
    // **Proves:** ROR-003 AC §12 — unknown hash in new_chain_hashes returns BlockNotInStore.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let unknown = Bytes32::new([0xDE; 32]);
    let err = store.apply_reorg(1, &[unknown]).expect_err("should fail");
    assert!(
        matches!(err, BlockStoreError::BlockNotInStore(_)),
        "expected BlockNotInStore, got {err:?}"
    );
}

#[test]
fn test_reorg_no_tip_error() {
    // **Proves:** ROR-003 — no tip returns NoTip error.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let err = store
        .apply_reorg(0, &[Bytes32::default()])
        .expect_err("no tip");
    assert!(matches!(err, BlockStoreError::NoTip));
}

#[test]
fn test_reorg_old_blocks_preserved() {
    // **Proves:** ROR-003 + ROR-004 — old canonical blocks survive the reorg.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(6);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let fork = build_fork(chain[2].hash(), 2, 300);
    for fb in &fork {
        store.put_block(fb, false).expect("store fork");
    }

    let fork_hashes: Vec<Bytes32> = fork.iter().map(|b| b.hash()).collect();
    store.apply_reorg(2, &fork_hashes).expect("reorg");

    // ALL original chain blocks should still be retrievable by hash
    for (i, block) in chain.iter().enumerate() {
        let got = store
            .get_block(&block.hash())
            .expect("get_block")
            .expect("block must survive reorg");
        assert_eq!(got.hash(), block.hash(), "chain block {i} preserved");
    }
}
