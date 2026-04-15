//! # PRN-004 — min_retained_height tracking via AtomicU64 + CF_METADATA persistence
//!
//! **Trace**
//! - Spec: [`PRN-004_min_retained_height_tracking.md`](../docs/requirements/domains/pruning/specs/PRN-004_min_retained_height_tracking.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::BlockStore;

use common::{build_chain, temp_blockstore_dir, test_config};

#[test]
fn test_fresh_store_min_is_zero() {
    // **Proves:** PRN-004 AC §6 — fresh database defaults to 0.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert_eq!(store.min_retained_height().expect("min"), 0);
}

#[test]
fn test_min_retained_height_survives_reopen() {
    // **Proves:** PRN-004 AC §1/§5 — persisted in CF_METADATA, loaded on startup.
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        let chain = build_chain(10);
        for block in &chain {
            store.extend_chain(block).expect("extend");
        }
        store.prune_before_height(5).expect("prune");
        assert_eq!(store.min_retained_height().expect("min"), 5);
    }
    // Reopen — should load from CF_METADATA
    let store2 = BlockStore::open(test_config(path)).expect("reopen");
    assert_eq!(
        store2.min_retained_height().expect("min"),
        5,
        "min_retained_height must survive reopen"
    );
}

#[test]
fn test_min_retained_height_monotonic() {
    // **Proves:** PRN-004 AC §7 — monotonically non-decreasing.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(20);
    for block in &chain {
        store.extend_chain(block).expect("extend");
    }
    store.prune_before_height(5).expect("prune to 5");
    assert_eq!(store.min_retained_height().expect("min"), 5);

    // Pruning below current min is a no-op
    let count = store.prune_before_height(3).expect("prune to 3");
    assert_eq!(count, 0, "no-op when height <= current min");
    assert_eq!(store.min_retained_height().expect("min"), 5);

    // Further pruning advances
    store.prune_before_height(10).expect("prune to 10");
    assert_eq!(store.min_retained_height().expect("min"), 10);
}
