//! # BLK-015 — Canonical [`BlockRecord`] range (`get_records_in_range`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-015.md`](../docs/requirements/domains/block_storage/specs/BLK-015.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-015)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-015-get-records-in-range-get_records_in_range)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | 1–2 | Inclusive range, ascending order | [`test_records_in_range_basic`], [`test_records_in_range_beyond_tip`] |
//! | 3 | Empty when `start > end` | [`test_records_in_range_empty`] |
//! | 4 | Skip missing canonical index | [`test_records_in_range_skips_canonical_gap`] |
//! | 5–6 | Header path only (no [`CF_BLOCKS`] physical reads on cold caches) | [`test_records_in_range_avoids_cf_blocks_on_miss`] |
//! | Fields | Identity matches headers / chain | [`test_records_in_range_fields_match_chain`] |
//!
//! **Instrumentation:** [`BlockStore::cf_blocks_physical_get_count`](dig_blockstore::BlockStore::cf_blocks_physical_get_count) vs
//! [`cf_headers_physical_get_count`](dig_blockstore::BlockStore::cf_headers_physical_get_count) prove the “lighter than
//! [`get_blocks_in_range`](dig_blockstore::BlockStore::get_blocks_in_range)” contract ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) vs [`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md)).

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

fn delete_cf_canonical_at_height(path: &Path, height: u64) {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors(&open_opts_write(), path, cfs).expect("open");
    let cf = db.cf_handle(CF_CANONICAL).expect("cf");
    db.delete_cf(cf, height_key(height).as_slice())
        .expect("delete canonical");
}

/// **Proves:** BLK-015 test plan `test_records_in_range_empty` — inverted bounds yield `Ok(vec![])` (NORMATIVE AC 3).
#[test]
fn test_records_in_range_empty() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let v = store.get_records_in_range(9, 1).expect("ok");
    assert!(v.is_empty());
}

/// **Proves:** BLK-015 test plan `test_records_in_range_basic` — five rows for heights `3..=7` on a dense `0..=9` chain.
#[test]
fn test_records_in_range_basic() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_records_in_range(3, 7).expect("range");
    assert_eq!(got.len(), 5);
    for (i, r) in got.iter().enumerate() {
        assert_eq!(r.height, 3 + i as u64);
    }
}

/// **Proves:** Heights past the stored tip contribute nothing (same truncation pattern as `tests/blk_014_tests.rs`).
#[test]
fn test_records_in_range_beyond_tip() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_records_in_range(0, 99).expect("range");
    assert_eq!(got.len(), 10);
    assert_eq!(got[9].height, 9);
}

/// **Proves:** NORMATIVE AC 4 — missing [`CF_CANONICAL`] row is skipped; remaining heights stay ordered.
#[test]
fn test_records_in_range_skips_canonical_gap() {
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
    let got = store.get_records_in_range(3, 7).expect("range");
    assert_eq!(got.len(), 4);
    assert_eq!(got[0].height, 3);
    assert_eq!(got[3].height, 7);
}

/// **Proves:** BLK-015 test plan `test_records_in_range_fields` — [`dig_blockstore::BlockRecord`] identity fields mirror the
/// corresponding [`dig_block::L2BlockHeader`] / [`L2Block::hash`](dig_block::L2Block::hash) from the fixture chain ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md)).
#[test]
fn test_records_in_range_fields_match_chain() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    let got = store.get_records_in_range(2, 4).expect("range");
    assert_eq!(got.len(), 3);
    for r in got {
        let h = r.height as usize;
        let b = &chain[h];
        assert_eq!(r.hash, b.hash());
        assert_eq!(r.height, b.height());
        assert_eq!(r.epoch, b.header.epoch);
        assert_eq!(r.parent_hash, b.header.parent_hash);
    }
}

/// **Proves:** AC 5–6 — With block, header, and record caches cold, [`get_records_in_range`] issues **only** [`CF_HEADERS`]
/// physical reads (five increments), while a follow-up [`get_blocks_in_range`] on the same cold caches issues five
/// [`CF_BLOCKS`] reads ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md) vs [`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)).
#[test]
fn test_records_in_range_avoids_cf_blocks_on_miss() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(10);
    for b in &chain {
        assert!(store.put_block(b, true).expect("put"));
    }
    for h in 3u64..=7u64 {
        let hash = chain[h as usize].hash();
        store.invalidate_block_cache_entry(&hash);
        store.invalidate_header_cache_entry(&hash);
        store.invalidate_record_cache_entry(&hash);
    }
    let blocks_before = store.cf_blocks_physical_get_count();
    let headers_before = store.cf_headers_physical_get_count();
    let recs = store.get_records_in_range(3, 7).expect("records");
    assert_eq!(recs.len(), 5);
    assert_eq!(
        store.cf_blocks_physical_get_count() - blocks_before,
        0,
        "get_records_in_range must not route through get_block/CF_BLOCKS on header miss"
    );
    assert_eq!(
        store.cf_headers_physical_get_count() - headers_before,
        5,
        "five cold headers loaded from CF_HEADERS"
    );
    for h in 3u64..=7u64 {
        let hash = chain[h as usize].hash();
        store.invalidate_block_cache_entry(&hash);
        store.invalidate_header_cache_entry(&hash);
        store.invalidate_record_cache_entry(&hash);
    }
    let blocks_mid = store.cf_blocks_physical_get_count();
    let _blocks = store.get_blocks_in_range(3, 7).expect("blocks");
    assert_eq!(
        store.cf_blocks_physical_get_count() - blocks_mid,
        5,
        "get_blocks_in_range must touch CF_BLOCKS once per height on cold block cache"
    );
}
