//! # BLK-014 — Canonical range materialization (`get_blocks_in_range`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-014.md`](../docs/requirements/domains/block_storage/specs/BLK-014.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-014)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-014-get-blocks-in-range-get_blocks_in_range)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | 1 | Inclusive `[start, end]` | [`test_blocks_in_range_basic`], [`test_blocks_in_range_single`] |
//! | 2 | Ascending height order | All range tests assert `windows` / consecutive heights |
//! | 3 | Empty when `start > end` | [`test_blocks_in_range_empty`] |
//! | 4 | Skip missing canonical rows | [`test_blocks_in_range_skips_canonical_gap`] |
//! | 5 | Beyond tip: no error, shorter vec | [`test_blocks_in_range_beyond_tip`] |
//! | 6 | Full block (decompress) per row | Same as [`get_block`](dig_blockstore::BlockStore::get_block) path — [`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) |
//!
//! **Fixture pattern:** Dense canonical chain via [`build_chain`] + [`BlockStore::put_block`] with `canonical: true`
//! matches `tests/blk_006_tests.rs` ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::height_key;
use dig_blockstore::{BlockStore, CF_CANONICAL};

use common::{build_chain, temp_blockstore_dir, test_config};

fn open_opts_write() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Delete one [`CF_CANONICAL`] row at `height` while leaving [`CF_BLOCKS`] intact — simulates a gap in the height index.
fn delete_cf_canonical_at_height(path: &Path, height: u64) {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors(&open_opts_write(), path, cfs).expect("open for gap");
    let cf = db.cf_handle(CF_CANONICAL).expect("cf canonical");
    db.delete_cf(cf, height_key(height).as_slice())
        .expect("delete_cf canonical row");
}

/// **Proves:** BLK-014 test plan `test_blocks_in_range_empty` — inverted bounds return an empty `Vec` without error.
#[test]
fn test_blocks_in_range_empty() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let v = store.get_blocks_in_range(7, 3).expect("query");
    assert!(v.is_empty());
}

/// **Proves:** BLK-014 test plan `test_blocks_in_range_basic` — heights `3..=7` on a `0..=9` canonical chain yield five blocks in order.
#[test]
fn test_blocks_in_range_basic() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_blocks_in_range(3, 7).expect("range");
    assert_eq!(got.len(), 5);
    for (i, b) in got.iter().enumerate() {
        assert_eq!(b.height(), 3 + i as u64);
    }
}

/// **Proves:** BLK-014 test plan `test_blocks_in_range_full` — entire chain `0..=9` returned in one call.
#[test]
fn test_blocks_in_range_full() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_blocks_in_range(0, 9).expect("range");
    assert_eq!(got.len(), 10);
    assert_eq!(got[0].height(), 0);
    assert_eq!(got[9].height(), 9);
}

/// **Proves:** BLK-014 test plan `test_blocks_in_range_beyond_tip` — requesting past the last canonical height simply
/// stops contributing rows (no panic, no error).
#[test]
fn test_blocks_in_range_beyond_tip() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_blocks_in_range(0, 20).expect("range");
    assert_eq!(got.len(), 10);
}

/// **Proves:** BLK-014 test plan `test_blocks_in_range_single` — degenerate range `h..=h` returns zero or one block.
#[test]
fn test_blocks_in_range_single() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_blocks_in_range(5, 5).expect("range");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].height(), 5);
}

/// **Proves:** AC 4 (NORMATIVE) — a missing [`CF_CANONICAL`] entry is skipped; surrounding heights still appear in order.
#[test]
fn test_blocks_in_range_skips_canonical_gap() {
    let (_guard, path) = temp_blockstore_dir();
    let cfg = test_config(path.clone());
    let store = BlockStore::open(cfg.clone()).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    drop(store);
    delete_cf_canonical_at_height(&path, 5);
    let store = BlockStore::open(cfg).expect("reopen");
    let got = store.get_blocks_in_range(3, 7).expect("range");
    assert_eq!(got.len(), 4);
    assert_eq!(got[0].height(), 3);
    assert_eq!(got[1].height(), 4);
    assert_eq!(got[2].height(), 6);
    assert_eq!(got[3].height(), 7);
}
