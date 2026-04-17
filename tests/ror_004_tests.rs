//! # ROR-004 — Fork preservation: non-canonical blocks remain accessible by hash
//!
//! **Trace**
//! - Spec: [`ROR-004.md`](../docs/requirements/domains/rollback_reorg/specs/ROR-004.md)
//! - NORMATIVE: [`NORMATIVE.md` (ROR-004)](../docs/requirements/domains/rollback_reorg/NORMATIVE.md)
//!
//! ## What this file proves
//!
//! Block data (CF_BLOCKS, CF_HEADERS) is NEVER deleted during rollback or reorg.
//! Only the canonical index (CF_CANONICAL) changes. This enables fast reorg without
//! re-downloading blocks from the network.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_blocks_survive_rollback() {
    // **Proves:** ROR-004 AC §1 — after rollback, all blocks still in CF_BLOCKS.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.rollback_to_height(5).expect("rollback");

    // ALL 10 blocks must still be retrievable by hash
    for (i, block) in chain.iter().enumerate() {
        let got = store
            .get_block(&block.hash())
            .expect("get_block")
            .expect("must survive rollback");
        assert_eq!(got.hash(), block.hash(), "block {i} preserved");
    }
}

#[test]
fn test_headers_survive_rollback() {
    // **Proves:** ROR-004 AC §2 — after rollback, all headers still in CF_HEADERS.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.rollback_to_height(3).expect("rollback");

    for (i, block) in chain.iter().enumerate() {
        let hdr = store
            .get_header(&block.hash())
            .expect("get_header")
            .expect("must survive rollback");
        assert_eq!(hdr, block.header, "header {i} preserved");
    }
}

#[test]
fn test_non_canonical_get_block() {
    // **Proves:** ROR-004 AC §5 — get_block works for non-canonical blocks.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    store.extend_chain(&chain[0]).expect("genesis");

    // Store block 1 as non-canonical
    store
        .put_block(&chain[1], false)
        .expect("put non-canonical");

    let got = store
        .get_block(&chain[1].hash())
        .expect("get")
        .expect("non-canonical block retrievable");
    assert_eq!(got.hash(), chain[1].hash());
}

#[test]
fn test_recanonicalize_existing_block() {
    // **Proves:** ROR-004 AC §8 — a rolled-back block can be re-canonicalized without re-storing.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Rollback to height 2 (reverts heights 3,4)
    store.rollback_to_height(2).expect("rollback");
    assert!(
        store.get_hash_by_height(3).expect("h3").is_none(),
        "height 3 no longer canonical"
    );

    // Re-canonicalize height 3 without re-storing
    store
        .set_canonical(&chain[3].hash())
        .expect("re-canonicalize");
    let h3 = store
        .get_hash_by_height(3)
        .expect("h3")
        .expect("re-canonical");
    assert_eq!(
        h3,
        chain[3].hash(),
        "block re-canonicalized from existing data"
    );
}

#[test]
fn test_block_count_unchanged_by_rollback() {
    // **Proves:** ROR-004 AC §7 — CF_BLOCKS row count unchanged by rollback.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    let stats_before = store.stats().expect("stats");
    store.rollback_to_height(3).expect("rollback");
    let stats_after = store.stats().expect("stats");

    assert_eq!(
        stats_before.block_count, stats_after.block_count,
        "block_count must not change on rollback (fork preservation)"
    );
}
