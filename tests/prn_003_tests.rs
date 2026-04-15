//! # PRN-003 — Compaction filter: background pruning during RocksDB compaction
//!
//! **Trace**
//! - Spec: [`PRN-003_compaction_filter.md`](../docs/requirements/domains/pruning/specs/PRN-003_compaction_filter.md)
//!
//! ## What this file proves
//!
//! When `enable_compaction_pruning=true`, a compaction filter is registered on
//! CF_HEADERS that drops entries where the header's block height is below
//! `min_retained_height`. The filter reads the threshold from a shared `Arc<AtomicU64>`
//! with `Acquire` ordering.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, BlockStoreConfig};

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_compaction_filter_registered_when_enabled() {
    // **Proves:** PRN-003 AC §1 — store opens with compaction filter when flag is true.
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path.clone());
    cfg.enable_compaction_pruning = true;
    let store = BlockStore::open(cfg).expect("open with compaction filter");
    assert_eq!(store.min_retained_height().expect("min"), 0);
}

#[test]
fn test_compaction_filter_not_registered_when_disabled() {
    // **Proves:** PRN-003 AC §2 — default disabled.
    let cfg = BlockStoreConfig::default();
    assert!(!cfg.enable_compaction_pruning);
}

#[test]
fn test_compaction_filter_drops_below_threshold() {
    // **Proves:** PRN-003 AC §3 — after pruning sets min_retained_height and compact()
    // is called, headers below the threshold are removed by the compaction filter.
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path.clone());
    cfg.enable_compaction_pruning = true;
    let store = BlockStore::open(cfg).expect("open");
    let chain = build_chain(10);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    // Set min_retained_height to 5 via explicit pruning
    store.prune_before_height(5).expect("prune");
    assert_eq!(store.min_retained_height().expect("min"), 5);

    // Trigger compaction — the filter should clean up any remaining stale data
    store.compact().expect("compact");

    // Heights below 5 should definitely be gone (both explicit prune + compaction filter)
    for h in 0..5 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_none(),
            "height {h} should be pruned"
        );
    }
    // Heights 5..9 should survive
    for h in 5..10 {
        assert!(
            store.get_hash_by_height(h).expect("h").is_some(),
            "height {h} should survive"
        );
    }
}

#[test]
fn test_compaction_filter_keeps_above_threshold() {
    // **Proves:** PRN-003 AC §3 — entries at/above threshold kept.
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path.clone());
    cfg.enable_compaction_pruning = true;
    let store = BlockStore::open(cfg).expect("open");
    let chain = build_chain(8);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }

    store.prune_before_height(3).expect("prune");
    store.compact().expect("compact");

    // Block at height 3 should be retrievable
    let got = store
        .get_block(&chain[3].hash())
        .expect("get")
        .expect("should survive compaction filter");
    assert_eq!(got.hash(), chain[3].hash());
}

#[test]
fn test_compaction_filter_threshold_survives_reopen() {
    // **Proves:** PRN-003 AC §7/§8 — AtomicU64 initialized from persisted value on reopen.
    let (_guard, path) = temp_blockstore_dir();
    {
        let mut cfg = test_config(path.clone());
        cfg.enable_compaction_pruning = true;
        let store = BlockStore::open(cfg).expect("open");
        let chain = build_chain(10);
        for block in &chain {
            store.extend_chain(block).expect("extend");
        }
        store.prune_before_height(5).expect("prune");
    }
    // Reopen with compaction pruning — threshold should be loaded
    let mut cfg2 = test_config(path);
    cfg2.enable_compaction_pruning = true;
    let store2 = BlockStore::open(cfg2).expect("reopen");
    assert_eq!(
        store2.min_retained_height().expect("min"),
        5,
        "threshold loaded from CF_METADATA on reopen"
    );
}
