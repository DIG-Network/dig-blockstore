//! # ROR-002 — `find_common_ancestor`: walk parent_hash chain to canonical fork point
//!
//! **Trace**
//! - Spec: [`ROR-002.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-002.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-002)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! `find_common_ancestor(hash, max_depth)` walks backward through `parent_hash` links,
//! checking at each step whether the block is the canonical block at its height. Returns
//! the first canonical match. This is the core primitive for reorg detection: when a
//! new block arrives whose parent is not the current tip, this function finds where the
//! fork diverged.
//!
//! ## Fork chain test strategy
//!
//! Tests build a **canonical** chain (heights 0..N) via `extend_chain`, then store
//! **fork** blocks that branch off at some height F using `put_block(fork_block, false)`.
//! Fork blocks have the same parent_hash as the canonical block at height F but a
//! different hash (different height or different content). Walking the fork chain backward
//! should find the canonical block at height F as the common ancestor.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// Build a fork chain of length `n` branching from `parent_hash` starting at height `start_height`.
/// Heights in the fork use `start_height + 1000 * (i+1)` to avoid collisions with canonical heights.
/// This produces blocks with unique hashes that link back to parent_hash.
fn build_fork(parent_hash: Bytes32, fork_len: usize, start_height: u64) -> Vec<dig_block::L2Block> {
    let mut blocks = Vec::with_capacity(fork_len);
    let mut parent = parent_hash;
    for i in 0..fork_len {
        // Use a distinct height to get a distinct hash from the canonical block
        let h = start_height + 1 + i as u64;
        let block = test_block(h, parent);
        parent = block.hash();
        blocks.push(block);
    }
    blocks
}

#[test]
fn test_find_ancestor_at_fork_point() {
    // **Proves:** ROR-002 AC §1 — fork tip finds the canonical block at the branch point.
    //
    // Setup: canonical chain 0..9, fork branches from height 5's parent (so fork blocks
    // share parent_hash with canonical[5]). Walking the fork backward should find
    // canonical[4] as the common ancestor.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Fork from height 4's hash (the parent of canonical height 5)
    let fork_parent = chain[4].hash();
    let fork = build_fork(fork_parent, 3, 100); // heights 101, 102, 103
    for fb in &fork {
        store.put_block(fb, false).expect("store fork block");
    }

    // Walk from fork tip — should find canonical[4] as the ancestor
    let result = store
        .find_common_ancestor(&fork[2].hash(), 100)
        .expect("find");
    let (ancestor_hash, ancestor_height) = result.expect("should find ancestor");
    assert_eq!(ancestor_hash, chain[4].hash());
    assert_eq!(ancestor_height, 4);
}

#[test]
fn test_find_ancestor_already_canonical() {
    // **Proves:** ROR-002 AC §2 — a canonical block hash returns itself immediately.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let result = store
        .find_common_ancestor(&chain[3].hash(), 100)
        .expect("find");
    let (hash, height) = result.expect("canonical block is its own ancestor");
    assert_eq!(hash, chain[3].hash());
    assert_eq!(height, 3);
}

#[test]
fn test_find_ancestor_not_in_store() {
    // **Proves:** ROR-002 AC §3 — unknown hash returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let unknown = Bytes32::new([0xDE; 32]);
    let result = store.find_common_ancestor(&unknown, 100).expect("find");
    assert!(result.is_none(), "unknown hash → None");
}

#[test]
fn test_find_ancestor_exceeds_depth() {
    // **Proves:** ROR-002 AC §4 — max_depth too small returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Fork from height 2, length 5 → fork tip is 5 steps from ancestor
    let fork = build_fork(chain[2].hash(), 5, 200);
    for fb in &fork {
        store.put_block(fb, false).expect("store fork");
    }

    // max_depth=3 is too small to walk 5 fork blocks + reach height 2
    let result = store
        .find_common_ancestor(&fork[4].hash(), 3)
        .expect("find");
    assert!(result.is_none(), "max_depth too small → None");
}

#[test]
fn test_find_ancestor_zero_depth() {
    // **Proves:** ROR-002 AC §5 — max_depth=0 always returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let result = store
        .find_common_ancestor(&chain[2].hash(), 0)
        .expect("find");
    assert!(result.is_none(), "max_depth=0 → None");
}

#[test]
fn test_find_ancestor_at_genesis() {
    // **Proves:** ROR-002 AC §8 — fork from genesis finds height 0 as ancestor.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Fork from genesis (height 0)
    let fork = build_fork(chain[0].hash(), 3, 300);
    for fb in &fork {
        store.put_block(fb, false).expect("store fork");
    }

    let result = store
        .find_common_ancestor(&fork[2].hash(), 100)
        .expect("find");
    let (hash, height) = result.expect("genesis is ancestor");
    assert_eq!(hash, chain[0].hash());
    assert_eq!(height, 0);
}

#[test]
fn test_find_ancestor_broken_chain() {
    // **Proves:** ROR-002 AC — missing parent in chain returns None.
    //
    // Store only the fork tip (not its parents), so the parent_hash walk hits
    // a missing block and returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Create a fork but only store the LAST block (gap in parent chain)
    let fork = build_fork(chain[1].hash(), 3, 400);
    store.put_block(&fork[2], false).expect("store only tip");
    // fork[2]'s parent is fork[1], which is NOT stored → broken chain

    let result = store
        .find_common_ancestor(&fork[2].hash(), 100)
        .expect("find");
    assert!(result.is_none(), "broken chain → None");
}
