//! # BLK-005 — `get_blocks_by_hash`: cache-first per hash, single `multi_get_cf` for misses
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-005.md`](../docs/requirements/domains/block_storage/specs/BLK-005.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-005)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-005-batch-retrieval-get_blocks_by_hash)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//! - Single-key precedent: [`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) (`get_block`)
//!
//! ## Proof strategy (maps to BLK-005 acceptance criteria)
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Every stored hash resolves to its block | [`test_batch_returns_all_stored_blocks`] |
//! | §2 | All cache hits ⇒ no batch RocksDB path | [`test_batch_all_cache_hits_skip_multi_get`] |
//! | §3 | All misses ⇒ one `multi_get_cf` batch | [`test_batch_all_misses_uses_single_multi_get_batch`] |
//! | §4 | Read-through repopulates LRU + header cache | [`test_batch_misses_populate_block_cache_for_followup_get_block`] |
//! | §5 | Output order matches input order | [`test_batch_ordering_matches_input_permutation`] |
//! | §6 | Unknown hash ⇒ `None` at that index, no overall error | [`test_batch_missing_hash_slot_is_none`] |
//!
//! **Instrumentation:** [`BlockStore::cf_blocks_multi_get_batch_count`](dig_blockstore::BlockStore::cf_blocks_multi_get_batch_count)
//! increments **once per [`get_blocks_by_hash`](dig_blockstore::BlockStore::get_blocks_by_hash) call** that had ≥1 cache miss
//! (one RocksDB `multi_get_cf` covering every miss in that call). [`cf_blocks_physical_get_count`](dig_blockstore::BlockStore::cf_blocks_physical_get_count)
//! remains tied to [`get_block`](dig_blockstore::BlockStore::get_block) only ([`tests/blk_002_tests.rs`](blk_002_tests.rs)).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::L2Block;
use dig_blockstore::BlockStore;

use common::{temp_blockstore_dir, test_block, test_config};

#[test]
fn test_batch_returns_all_stored_blocks() {
    // **Proves:** AC §1 — every hash that exists in the store must yield `Some(block)` with matching [`L2Block::hash`].
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b1 = test_block(1, ZERO_HASH);
    let b2 = test_block(2, b1.hash());
    let b3 = test_block(3, b2.hash());
    assert!(store.put_block(&b1, false).expect("p1"));
    assert!(store.put_block(&b2, false).expect("p2"));
    assert!(store.put_block(&b3, false).expect("p3"));
    let hashes = [b1.hash(), b2.hash(), b3.hash()];
    let got = store.get_blocks_by_hash(&hashes).expect("batch");
    assert_eq!(got.len(), 3);
    for (exp, slot) in [&b1, &b2, &b3].into_iter().zip(got.iter()) {
        let b = slot.as_ref().expect("present");
        assert_eq!(b.hash(), exp.hash());
    }
}

#[test]
fn test_batch_all_cache_hits_skip_multi_get() {
    // **Proves:** AC §2 — after [`put_block`] warms [`BlockStore::block_cache`], a batch read must not increment
    // [`cf_blocks_multi_get_batch_count`] (no `multi_get_cf` on the miss path).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(8, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
    let got = store
        .get_blocks_by_hash(&[b.hash(), b.hash()])
        .expect("batch");
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].as_ref().expect("a").hash(), b.hash());
    assert_eq!(got[1].as_ref().expect("b").hash(), b.hash());
}

#[test]
fn test_batch_all_misses_uses_single_multi_get_batch() {
    // **Proves:** AC §3 — with every requested hash cold in the LRU, exactly **one** batch counter increment covers
    // **all** misses (single `multi_get_cf` invocation inside the implementation).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let blocks: [L2Block; 4] = {
        let b0 = test_block(10, ZERO_HASH);
        let b1 = test_block(11, b0.hash());
        let b2 = test_block(12, b1.hash());
        let b3 = test_block(13, b2.hash());
        [b0, b1, b2, b3]
    };
    for b in &blocks {
        assert!(store.put_block(b, false).expect("put"));
        store.invalidate_block_cache_entry(&b.hash());
    }
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
    let hashes: Vec<_> = blocks.iter().map(L2Block::hash).collect();
    let got = store.get_blocks_by_hash(&hashes).expect("batch");
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 1);
    assert_eq!(got.len(), 4);
    for (exp, slot) in blocks.iter().zip(got.iter()) {
        assert_eq!(slot.as_ref().expect("dec").hash(), exp.hash());
    }
}

#[test]
fn test_batch_mixed_hit_miss_single_multi_get() {
    // **Proves:** AC §2 + §3 + §5 — a mix of warm and cold hashes: warm rows never force an extra batch; all cold rows
    // share one `multi_get_cf`; returned [`Vec`] stays aligned with the requested order `[miss, hit, miss]`.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b1 = test_block(20, ZERO_HASH);
    let b2 = test_block(21, b1.hash());
    let b3 = test_block(22, b2.hash());
    store.put_block(&b1, false).expect("p1");
    store.put_block(&b2, false).expect("p2");
    store.put_block(&b3, false).expect("p3");
    store.invalidate_block_cache_entry(&b1.hash());
    store.invalidate_block_cache_entry(&b3.hash());
    // b2 remains cached
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
    let hashes = [b1.hash(), b2.hash(), b3.hash()];
    let got = store.get_blocks_by_hash(&hashes).expect("batch");
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 1);
    assert_eq!(got[0].as_ref().expect("b1").hash(), b1.hash());
    assert_eq!(got[1].as_ref().expect("b2").hash(), b2.hash());
    assert_eq!(got[2].as_ref().expect("b3").hash(), b3.hash());
}

#[test]
fn test_batch_ordering_matches_input_permutation() {
    // **Proves:** AC §5 — permuted request order must yield the **same permutation** of blocks (compare hashes slot-wise).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b1 = test_block(30, ZERO_HASH);
    let b2 = test_block(31, b1.hash());
    let b3 = test_block(32, b2.hash());
    for b in [&b1, &b2, &b3] {
        store.put_block(b, false).expect("put");
    }
    let hashes = [b3.hash(), b1.hash(), b2.hash()];
    let got = store.get_blocks_by_hash(&hashes).expect("batch");
    assert_eq!(got[0].as_ref().expect("").hash(), b3.hash());
    assert_eq!(got[1].as_ref().expect("").hash(), b1.hash());
    assert_eq!(got[2].as_ref().expect("").hash(), b2.hash());
}

#[test]
fn test_batch_missing_hash_slot_is_none() {
    // **Proves:** AC §6 — interleave a never-stored hash; that index is `None`, others still `Some`; one batch read still suffices.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(40, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    store.invalidate_block_cache_entry(&b.hash());
    let ghost = Bytes32::new([0xEE; 32]);
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
    let got = store.get_blocks_by_hash(&[b.hash(), ghost]).expect("batch");
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 1);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].as_ref().expect("real").hash(), b.hash());
    assert!(got[1].is_none());
}

#[test]
fn test_batch_misses_populate_block_cache_for_followup_get_block() {
    // **Proves:** AC §4 — each deserialized block is inserted into the shared LRU so a subsequent [`get_block`] is a hit
    // (physical `get_cf` counter for single-key path stays zero after batch warm).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(50, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    store.invalidate_block_cache_entry(&b.hash());
    store.get_blocks_by_hash(&[b.hash()]).expect("batch");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    store.get_block(&b.hash()).expect("follow").expect("some");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
}

#[test]
fn test_batch_empty_input_no_rocksdb_side_effects() {
    // **Edge:** empty slice ⇒ empty result, no batch counter increment (no `multi_get_cf` with zero keys).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let got = store.get_blocks_by_hash(&[]).expect("empty");
    assert!(got.is_empty());
    assert_eq!(store.cf_blocks_multi_get_batch_count(), 0);
}
