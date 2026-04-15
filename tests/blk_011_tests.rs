//! # BLK-011 — Lightweight existence (`has_block` by hash)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-011.md`](../docs/requirements/domains/block_storage/specs/BLK-011.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-011)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-011-has-block-has_block)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | 1 | `true` after [`BlockStore::put_block`] | [`test_has_block_true_after_put`] |
//! | 4 | `false` for unknown hash | [`test_has_block_false_for_unknown_hash`] |
//! | 3 | No deserialize/decompress (no [`get_block`] counter bump on pure cache path) | [`test_has_block_cache_hit_skips_get_block_physical_counter`] |
//! | 2 | Cache consulted before RocksDB | [`test_has_block_header_cache_suffices_after_block_evict`] |
//!
//! **Counter contract:** [`BlockStore::cf_blocks_physical_get_count`] increments **only** inside [`BlockStore::get_block`]
//! on cache miss ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)); `has_block` must not trip that counter when caches satisfy the query.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use dig_block::constants::ZERO_HASH;
use dig_blockstore::BlockStore;

use common::{temp_blockstore_dir, test_block, test_config};

/// **Proves:** AC 1 — a row written by [`BlockStore::put_block`] is visible to [`BlockStore::has_block`].
#[test]
fn test_has_block_true_after_put() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(2, ZERO_HASH);
    let h = b.hash();
    store.put_block(&b, false).expect("put");
    assert!(store.has_block(&h).expect("has_block"));
}

/// **Proves:** AC 4 — random hash with no rows returns `false`.
#[test]
fn test_has_block_false_for_unknown_hash() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let unknown = test_block(77, ZERO_HASH).hash();
    assert!(!store.has_block(&unknown).expect("has_block"));
}

/// **Proves:** AC 2 + 3 — with both body and header LRUs populated, [`has_block`] returns `true` while
/// [`BlockStore::cf_blocks_physical_get_count`] stays at zero (no [`get_block`] miss path, no decode path).
#[test]
fn test_has_block_cache_hit_skips_get_block_physical_counter() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(5, ZERO_HASH);
    let h = b.hash();
    store.put_block(&b, false).expect("put");
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    assert!(store.has_block(&h).expect("has_block"));
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        0,
        "has_block must not increment the get_block physical-read counter"
    );
}

/// **Proves:** AC 2 — evict only the block body from the sharded block LRU;
/// a warm header cache entry is enough for [`has_block`] to return `true` **without** incrementing the block-cache miss counter.
#[test]
fn test_has_block_header_cache_suffices_after_block_evict() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(9, ZERO_HASH);
    let h = b.hash();
    store.put_block(&b, false).expect("put");
    store.invalidate_block_cache_entry(&h);
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    assert!(
        store.has_block(&h).expect("has_block"),
        "header cache still holds the key after block LRU eviction"
    );
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        0,
        "header-cache hit must not route through get_block's CF_BLOCKS counter"
    );
}

/// **Proves:** Cold path after both caches evicted — [`has_block`] still returns `true` via RocksDB header probe
/// (and still does not use [`get_block`], so the physical counter remains zero).
#[test]
fn test_has_block_cold_path_after_full_cache_evict() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(11, ZERO_HASH);
    let h = b.hash();
    store.put_block(&b, false).expect("put");
    store.invalidate_block_cache_entry(&h);
    store.invalidate_header_cache_entry(&h);
    assert_eq!(store.cf_blocks_physical_get_count(), 0);
    assert!(store.has_block(&h).expect("has_block"));
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        0,
        "existence probe must not be implemented as get_block"
    );
}
