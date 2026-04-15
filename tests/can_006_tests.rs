//! # CAN-006 — `get_hash_by_height`: O(1) mmap lookup with CF_CANONICAL fallback
//!
//! **Trace**
//! - Spec + test plan: [`CAN-006.md`](../docs/requirements/domains/canonical_chain/specs/CAN-006.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-006)](../docs/requirements/domains/canonical_chain/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! `get_hash_by_height` is the foundation for all height-based lookups in the store.
//! Every higher-level method (`get_block_by_height`, `get_header_by_height`,
//! `get_record_by_height`, `get_epoch_block_hashes`) delegates through it. These tests
//! prove:
//!
//! 1. **Correct resolution** — canonical blocks stored via `put_block(b, true)` are
//!    retrievable by their height, returning the correct hash.
//! 2. **Non-existent height** — heights above the chain or never-canonicalized return `None`.
//! 3. **Dual-layer path** — mmap (`canonical.bin`) is consulted first; CF_CANONICAL
//!    provides the fallback when mmap is unavailable or doesn't cover the height.
//! 4. **Derived convenience methods** — `get_header_by_height` returns the full header,
//!    `get_record_by_height` returns the derived `BlockRecord`.
//! 5. **Epoch range** — `get_epoch_block_hashes` uses `dig_epoch::epoch_height_range`
//!    to collect hashes for a complete or partial epoch.
//!
//! ## Chia analogy
//!
//! Chia's `Blockchain.height_to_hash(height)` reads from an in-memory `BlockHeightMap`
//! (a Python `bytearray`). DIG's `get_hash_by_height` reads from a memory-mapped file
//! (`canonical.bin`) with the same O(1) offset calculation (`height * 32`), plus a
//! durable RocksDB fallback that Chia lacks.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_block::BlockStatus;
use dig_blockstore::{BlockRecord, BlockStore};

use common::{build_chain, temp_blockstore_dir, test_config};

// ---------------------------------------------------------------------------
// Core: get_hash_by_height
// ---------------------------------------------------------------------------

#[test]
fn test_get_hash_by_height_found() {
    // **Proves:** CAN-006 AC §1 — correct hash returned for a canonical height.
    //
    // **Requirement complete when:** After storing 5 canonical blocks via init_genesis +
    // put_block(canonical=true), `get_hash_by_height(h)` returns the correct hash for
    // each height 0..4.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);

    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        assert!(store.put_block(block, true).expect("put"));
    }

    for (i, block) in chain.iter().enumerate() {
        let hash = store
            .get_hash_by_height(i as u64)
            .expect("get_hash_by_height")
            .expect("should be Some for canonical height");
        assert_eq!(
            hash,
            block.hash(),
            "hash at height {i} must match the stored block"
        );
    }
}

#[test]
fn test_get_hash_by_height_not_found() {
    // **Proves:** CAN-006 AC §2 — None for a height with no canonical block.
    //
    // **Requirement complete when:** Querying height 999 on a store with only 3 blocks
    // returns `Ok(None)`.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    assert!(
        store
            .get_hash_by_height(999)
            .expect("get_hash_by_height")
            .is_none(),
        "height 999 has no canonical block"
    );
}

#[test]
fn test_get_hash_by_height_empty_store() {
    // **Proves:** CAN-006 — empty store returns None for any height.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert!(store.get_hash_by_height(0).expect("h0").is_none());
    assert!(store.get_hash_by_height(1).expect("h1").is_none());
}

#[test]
fn test_rocksdb_fallback_when_mmap_disabled() {
    // **Proves:** CAN-006 AC §4 — RocksDB fallback works when mmap is unavailable.
    //
    // **Requirement complete when:** After disabling the canonical.bin acceleration
    // (via `disable_canonical_bin_acceleration`), `get_hash_by_height` still resolves
    // the correct hash via the CF_CANONICAL column family.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    // Disable mmap — forces CF_CANONICAL path
    store.disable_canonical_bin_acceleration();

    for (i, block) in chain.iter().enumerate() {
        let hash = store
            .get_hash_by_height(i as u64)
            .expect("cf fallback")
            .expect("should resolve via CF_CANONICAL");
        assert_eq!(hash, block.hash(), "CF_CANONICAL fallback at height {i}");
    }
}

// ---------------------------------------------------------------------------
// Derived convenience methods
// ---------------------------------------------------------------------------

#[test]
fn test_get_header_by_height() {
    // **Proves:** CAN-006 AC §6 — `get_header_by_height` returns the correct header.
    //
    // **Requirement complete when:** The header returned by `get_header_by_height(h)`
    // is equal (via PartialEq) to the header embedded in the block stored at height `h`.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(4);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    for (i, block) in chain.iter().enumerate() {
        let header = store
            .get_header_by_height(i as u64)
            .expect("get_header_by_height")
            .expect("should be Some");
        assert_eq!(header, block.header, "header at height {i}");
    }
}

#[test]
fn test_get_header_by_height_not_found() {
    // **Proves:** CAN-006 AC §8 — convenience methods return None for non-existent heights.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert!(store.get_header_by_height(0).expect("h").is_none());
}

#[test]
fn test_get_record_by_height_delegates() {
    // **Proves:** CAN-006 AC §7 — `get_record_by_height` returns a valid BlockRecord
    // derived from the header at that height.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    let rec = store
        .get_record_by_height(2)
        .expect("get_record_by_height")
        .expect("should be Some");
    let expected = BlockRecord::from_header(&chain[2].header, BlockStatus::Validated);
    assert_eq!(rec, expected);
}

// ---------------------------------------------------------------------------
// Epoch block hashes
// ---------------------------------------------------------------------------

#[test]
fn test_get_epoch_block_hashes_full_epoch() {
    // **Proves:** CAN-006 AC §9 — correct hashes for a complete epoch.
    //
    // **Requirement complete when:** For a chain of 32+ blocks (one full epoch with
    // BLOCKS_PER_EPOCH=32), `get_epoch_block_hashes(0)` returns exactly 32 hashes
    // matching the canonical blocks at heights 0..31.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let epoch_size = dig_epoch::BLOCKS_PER_EPOCH as usize;
    let chain = build_chain(epoch_size + 5); // Full epoch 0 + partial epoch 1
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    let hashes = store.get_epoch_block_hashes(0).expect("epoch 0");
    assert_eq!(
        hashes.len(),
        epoch_size,
        "epoch 0 should have {epoch_size} hashes"
    );
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(*hash, chain[i].hash(), "epoch 0 hash at offset {i}");
    }
}

#[test]
fn test_get_epoch_block_hashes_partial_epoch() {
    // **Proves:** CAN-006 AC §10 — partial results for an incomplete epoch.
    //
    // **Requirement complete when:** For a chain of 5 blocks (heights 0..4),
    // `get_epoch_block_hashes(0)` returns exactly 5 hashes (not 32), stopping
    // at the chain tip without error.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(5);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    let hashes = store.get_epoch_block_hashes(0).expect("epoch 0 partial");
    assert_eq!(hashes.len(), 5, "only 5 blocks stored in epoch 0");
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(*hash, chain[i].hash());
    }
}

#[test]
fn test_get_epoch_block_hashes_empty_epoch() {
    // **Proves:** CAN-006 — epoch with no blocks returns empty Vec.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");
    for block in &chain[1..] {
        store.put_block(block, true).expect("put");
    }

    // Epoch 99 starts at height 99*32 = 3168, well beyond our 3-block chain
    let hashes = store.get_epoch_block_hashes(99).expect("epoch 99");
    assert!(hashes.is_empty(), "epoch 99 has no blocks");
}
