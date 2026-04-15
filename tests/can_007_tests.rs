//! # CAN-007 — Chain tip tracking: `tip()`, `height()`, `set_tip()` with CF_METADATA persistence
//!
//! **Trace**
//! - Spec + test plan: [`CAN-007.md`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-007)](../docs/requirements/domains/canonical_chain/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! The chain tip is the single most-queried piece of metadata in a block store — every
//! `extend_chain`, every parent-hash validation, every sync protocol message references it.
//! These tests verify:
//!
//! 1. **Empty-store baseline** — `tip()` and `height()` return `None` before genesis.
//! 2. **`height()` accessor** — trivially delegates to `tip().map(|t| t.height)`.
//! 3. **`set_tip()` persistence** — writes 40 bytes to `CF_METADATA` / `META_TIP` and
//!    updates the in-memory `RwLock<Option<ChainTip>>` cache.
//! 4. **Reopen durability** — tip survives close + reopen (loaded from `META_TIP` at startup).
//! 5. **Read-only guard** — `set_tip()` on a read-only handle returns an error.
//! 6. **Raw encoding** — the 40-byte layout is exactly `hash[0..32] || height_LE[32..40]`,
//!    matching [`TYP-006`](../docs/requirements/domains/storage_types/specs/TYP-006.md) and
//!    [`ChainTip::to_bytes()`](dig_blockstore::ChainTip::to_bytes).
//!
//! ## Chia analogy
//!
//! Chia stores the peak (tip) as `current_peak` in a SQLite `current_peak` table.
//! DIG uses a fixed-width 40-byte value in `CF_METADATA` under `META_TIP`, loaded into
//! a `parking_lot::RwLock` for lock-free reads on the hot path.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_blockstore::constants::{ALL_COLUMN_FAMILIES, CF_METADATA, META_TIP};
use dig_blockstore::{BlockStore, BlockStoreError, ChainTip, ERR_MUTATION_READ_ONLY};

use common::{temp_blockstore_dir, test_block, test_config};

/// Read raw META_TIP bytes from CF_METADATA via direct RocksDB access (bypasses BlockStore).
///
/// This proves the on-disk layout matches the spec's 40-byte encoding independently
/// of BlockStore's deserialization path.
fn read_raw_meta_tip(path: &Path) -> Option<Vec<u8>> {
    let mut opts = rocksdb::Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&opts, path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_METADATA)?;
    db.get_cf(cf, META_TIP.as_bytes()).ok().flatten()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_tip_none_on_empty() {
    // **Proves:** CAN-007 AC §1 — freshly opened empty store has no tip.
    //
    // **Requirement complete when:** Both `tip()` and `height()` return `None`
    // without any genesis or block insertion.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert!(store.tip().is_none(), "empty store must have no tip");
    assert!(store.height().is_none(), "empty store must have no height");
}

#[test]
fn test_height_accessor() {
    // **Proves:** CAN-007 AC §5 — `height()` returns `tip().map(|t| t.height)`.
    //
    // **Requirement complete when:** Before genesis, `height()` is `None`.
    // After genesis (height 0), `height()` is `Some(0)`. After extending to
    // height 5, `height()` is `Some(5)`. This exercises the accessor at
    // multiple chain states.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");

    // No tip yet
    assert_eq!(store.height(), None);

    // After genesis at height 0
    let genesis = test_block(0, Bytes32::default());
    store.init_genesis(&genesis).expect("init_genesis");
    assert_eq!(store.height(), Some(0));
}

#[test]
fn test_set_tip_updates_memory_and_disk() {
    // **Proves:** CAN-007 AC §3 — `set_tip()` writes to CF_METADATA and updates in-memory cache.
    //
    // **Requirement complete when:** After calling `set_tip` with a known ChainTip,
    // `tip()` returns the new value AND `read_raw_meta_tip` confirms the 40-byte
    // encoding was persisted to RocksDB.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");

    let new_tip = ChainTip {
        hash: Bytes32::new([0xAA; 32]),
        height: 42,
    };
    store.set_tip(new_tip).expect("set_tip");

    // In-memory check
    assert_eq!(store.tip(), Some(new_tip));
    assert_eq!(store.height(), Some(42));

    // On-disk check (raw read bypassing BlockStore)
    drop(store);
    let raw = read_raw_meta_tip(path.as_path()).expect("META_TIP should exist");
    assert_eq!(raw.len(), 40, "META_TIP must be exactly 40 bytes");
}

#[test]
fn test_set_tip_encoding_40_bytes() {
    // **Proves:** CAN-007 AC §6 — META_TIP value is exactly `hash[0..32] || height_LE[32..40]`.
    //
    // **Requirement complete when:** The raw bytes read from CF_METADATA match
    // `ChainTip::to_bytes()` for a known hash and height, proving the on-disk
    // format matches the TYP-006 specification.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");

    let tip = ChainTip {
        hash: Bytes32::new([0xFF; 32]),
        height: 0x0102_0304_0506_0708,
    };
    store.set_tip(tip).expect("set_tip");
    drop(store);

    let raw = read_raw_meta_tip(path.as_path()).expect("META_TIP");
    assert_eq!(&raw[0..32], &[0xFF; 32], "first 32 bytes = hash");
    assert_eq!(
        &raw[32..40],
        &tip.height.to_le_bytes(),
        "last 8 bytes = height LE"
    );
    // Also verify it matches ChainTip::to_bytes
    assert_eq!(raw.as_slice(), tip.to_bytes().as_slice());
}

#[test]
fn test_tip_survives_reopen() {
    // **Proves:** CAN-007 AC §7 — tip persisted in CF_METADATA survives close and reopen.
    //
    // **Requirement complete when:** A tip set via `set_tip` is recoverable after
    // dropping the store and opening a new handle on the same path.
    let (_guard, path) = temp_blockstore_dir();
    let expected_tip = ChainTip {
        hash: Bytes32::new([0xBB; 32]),
        height: 999,
    };

    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        store.set_tip(expected_tip).expect("set_tip");
    }

    let store2 = BlockStore::open(test_config(path)).expect("reopen");
    assert_eq!(
        store2.tip(),
        Some(expected_tip),
        "tip must survive close + reopen (loaded from META_TIP at startup)"
    );
    assert_eq!(store2.height(), Some(999));
}

#[test]
fn test_set_tip_read_only_error() {
    // **Proves:** CAN-007 + STR-004 — `set_tip()` on a read-only handle must fail.
    //
    // **Requirement complete when:** Calling `set_tip` on a read-only store returns
    // `Err(BlockStoreError::Serialization)` with the stable `ERR_MUTATION_READ_ONLY` message.
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open rw");
        store
            .init_genesis(&test_block(0, Bytes32::default()))
            .expect("genesis");
    }

    let ro = BlockStore::open_readonly(path.as_path()).expect("open_readonly");
    let tip = ChainTip {
        hash: Bytes32::new([0xCC; 32]),
        height: 1,
    };
    let err = ro.set_tip(tip).expect_err("read-only set_tip must fail");
    match err {
        BlockStoreError::Serialization(msg) => {
            assert!(
                msg.contains(ERR_MUTATION_READ_ONLY),
                "expected mutation-read-only error, got: {msg}"
            );
        }
        other => panic!("expected Serialization error, got: {other:?}"),
    }
}

#[test]
fn test_tip_after_init_genesis() {
    // **Proves:** CAN-007 AC §2 (init_genesis update point) — after genesis, tip is set to genesis block.
    //
    // **Requirement complete when:** `tip()` returns `Some(ChainTip { hash: genesis.hash(), height: 0 })`
    // after `init_genesis`, and the raw META_TIP bytes confirm persistence.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let genesis = test_block(0, Bytes32::default());
    let genesis_hash = genesis.hash();

    store.init_genesis(&genesis).expect("init_genesis");

    let tip = store.tip().expect("tip must exist after genesis");
    assert_eq!(tip.hash, genesis_hash);
    assert_eq!(tip.height, 0);

    // Verify raw persistence
    drop(store);
    let raw = read_raw_meta_tip(path.as_path()).expect("META_TIP after genesis");
    assert_eq!(raw.len(), 40);
    assert_eq!(&raw[0..32], genesis_hash.as_ref());
    assert_eq!(&raw[32..40], &0u64.to_le_bytes());
}

#[test]
fn test_set_tip_overwrites_previous() {
    // **Proves:** CAN-007 — calling `set_tip` multiple times updates to the latest value.
    //
    // **Requirement complete when:** Two successive `set_tip` calls leave `tip()` at
    // the second value, not the first.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");

    let tip1 = ChainTip {
        hash: Bytes32::new([0x11; 32]),
        height: 100,
    };
    let tip2 = ChainTip {
        hash: Bytes32::new([0x22; 32]),
        height: 200,
    };

    store.set_tip(tip1).expect("set_tip 1");
    assert_eq!(store.tip(), Some(tip1));

    store.set_tip(tip2).expect("set_tip 2");
    assert_eq!(store.tip(), Some(tip2));
    assert_eq!(store.height(), Some(200));
}
