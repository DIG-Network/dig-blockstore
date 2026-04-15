//! # CAC-006 — Cache warming on startup: preload recent canonical blocks into all caches
//!
//! **Trace**
//! - Spec: [`CAC-006_cache_warming_on_startup.md`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAC-006)](../docs/requirements/domains/caching/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/caching/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! Cache warming preloads recent canonical blocks into ALL in-memory caches during
//! `BlockStore::open()`, so the first N block requests after startup are cache hits
//! instead of cold RocksDB reads. These tests prove:
//!
//! 1. **Enabled** — when `warm_cache_on_open=true`, caches are populated before `open()` returns.
//! 2. **Disabled** — when `warm_cache_on_open=false`, caches start empty.
//! 3. **Cache hit immediately** — after warming, `get_block` for recent heights is a cache
//!    hit (zero `cf_blocks_physical_get_count`).
//! 4. **Empty store** — warming on an empty store succeeds with 0 blocks loaded.
//! 5. **Warm count** — `warm_blocks_loaded_count()` reports the actual number of blocks warmed.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

/// Create a store, fill it with `n` canonical blocks, then close it.
/// Returns the path for reopening.
fn fill_and_close(n: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let (guard, path) = temp_blockstore_dir();
    {
        let mut cfg = test_config(path.clone());
        cfg.warm_cache_on_open = false; // Don't warm on first open
        let store = BlockStore::open(cfg).expect("open");
        let chain = build_chain(n);
        for block in &chain {
            store.extend_chain(block).expect("extend");
        }
    }
    (guard, path)
}

#[test]
fn test_warming_enabled_populates_caches() {
    // **Proves:** CAC-006 AC §1/§5/§7 — warming populates caches, completes before open() returns.
    //
    // **Requirement complete when:** After reopening with warm_cache_on_open=true,
    // get_block for the tip block is a cache hit (cf_blocks_physical_get_count == 0).
    let (_guard, path) = fill_and_close(10);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 64; // More than the 10 blocks we have
    cfg.block_cache_capacity = 100; // Large enough to hold all 10 without shard eviction
    cfg.header_cache_capacity = 100;
    let store = BlockStore::open(cfg).expect("reopen with warming");

    // Warming should have loaded all 10 blocks
    assert!(
        store.warm_blocks_loaded_count() >= 10,
        "expected >= 10 warmed, got {}",
        store.warm_blocks_loaded_count()
    );

    // Recent block should be a cache hit — the physical read counter reflects
    // warming reads, but NO additional reads should happen for a warmed block.
    let tip = store.tip().expect("tip");
    let before = store.cf_blocks_physical_get_count();
    let got = store
        .get_block(&tip.hash)
        .expect("get_block")
        .expect("cached");
    assert_eq!(got.hash(), tip.hash);
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        before,
        "warmed block must be a cache hit — no additional RocksDB read"
    );
}

#[test]
fn test_warming_disabled_caches_empty() {
    // **Proves:** CAC-006 AC §2 — warm_cache_on_open=false leaves caches empty.
    //
    // **Requirement complete when:** After reopening with warming disabled,
    // get_block requires a physical RocksDB read (cache miss).
    let (_guard, path) = fill_and_close(5);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = false;
    let store = BlockStore::open(cfg).expect("reopen without warming");

    assert_eq!(store.warm_blocks_loaded_count(), 0);

    // First block read should be a cache miss
    let tip = store.tip().expect("tip");
    let _ = store.get_block(&tip.hash).expect("get_block");
    assert!(
        store.cf_blocks_physical_get_count() > 0,
        "without warming, first read must hit RocksDB"
    );
}

#[test]
fn test_warming_empty_store() {
    // **Proves:** CAC-006 AC §9 — empty store warming succeeds without error.
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 64;
    let store = BlockStore::open(cfg).expect("open empty with warming");
    assert_eq!(store.warm_blocks_loaded_count(), 0);
    assert!(store.tip().is_none());
}

#[test]
fn test_warming_count_matches_depth() {
    // **Proves:** CAC-006 AC §3/§4 — warming reads backward from tip, up to warm_cache_depth blocks.
    //
    // **Strategy:** Store 20 blocks, reopen with warm_cache_depth=5.
    // Only the 5 most recent blocks should be warmed.
    let (_guard, path) = fill_and_close(20);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 5;
    let store = BlockStore::open(cfg).expect("reopen");

    assert_eq!(
        store.warm_blocks_loaded_count(),
        5,
        "should warm exactly warm_cache_depth blocks"
    );

    // Tip block should be a cache hit (no additional reads beyond warming)
    let tip = store.tip().expect("tip");
    let before = store.cf_blocks_physical_get_count();
    store
        .get_block(&tip.hash)
        .expect("get")
        .expect("cached tip");
    assert_eq!(
        store.cf_blocks_physical_get_count(),
        before,
        "warmed tip must be a cache hit"
    );
}

#[test]
fn test_warming_fewer_blocks_than_depth() {
    // **Proves:** CAC-006 AC — when chain has fewer blocks than warm_cache_depth,
    // all available blocks are loaded.
    let (_guard, path) = fill_and_close(3);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 100; // Much larger than 3 blocks
    let store = BlockStore::open(cfg).expect("reopen");

    assert_eq!(
        store.warm_blocks_loaded_count(),
        3,
        "should warm all 3 available blocks"
    );
}

#[test]
fn test_warming_header_cache_populated() {
    // **Proves:** CAC-006 AC §5 — header cache also populated during warming.
    //
    // **Requirement complete when:** After warming, get_header for a recent block
    // is a cache hit (cf_headers_physical_get_count == 0).
    let (_guard, path) = fill_and_close(5);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 10;
    let store = BlockStore::open(cfg).expect("reopen");

    let tip = store.tip().expect("tip");
    let before = store.cf_headers_physical_get_count();
    let _ = store.get_header(&tip.hash).expect("get_header");
    assert_eq!(
        store.cf_headers_physical_get_count(),
        before,
        "header should be warmed into cache — no additional read"
    );
}

#[test]
fn test_warming_record_cache_populated() {
    // **Proves:** CAC-006 AC §5 — record cache also populated during warming.
    let (_guard, path) = fill_and_close(5);

    let mut cfg = test_config(path);
    cfg.warm_cache_on_open = true;
    cfg.warm_cache_depth = 10;
    let store = BlockStore::open(cfg).expect("reopen");

    // get_record for a recently warmed block should not need CF_HEADERS
    let tip = store.tip().expect("tip");
    let before = store.cf_headers_physical_get_count();
    let _ = store.get_record(&tip.hash).expect("get_record");
    assert_eq!(
        store.cf_headers_physical_get_count(),
        before,
        "record should be warmed into cache — no additional CF_HEADERS read"
    );
}
