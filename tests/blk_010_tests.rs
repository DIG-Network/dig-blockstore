//! # BLK-010 — Status updates (`update_status` on in-memory [`BlockRecord`] only)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-010.md`](../docs/requirements/domains/block_storage/specs/BLK-010.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-010)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-010-status-updates-update_status)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Cache row gets new [`BlockStatus`] | [`test_update_status_reflected_in_get_record`] |
//! | §2 | No RocksDB column family mutation | [`test_update_status_does_not_change_cf_headers_or_cf_blocks`] |
//! | §3 | Error when hash absent from record cache | [`test_update_status_unknown_hash_errors`], [`test_update_status_after_invalidate_errors`] |
//! | §4 | Subsequent [`get_record`] sees new status | [`test_update_status_reflected_in_get_record`], [`test_update_status_sequential_updates`] |
//!
//! **Error shape:** [`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) caps
//! [`BlockStoreError`] at thirteen variants, so “not cached” maps to [`BlockStoreError::Serialization`] with prefix
//! [`ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX`](dig_blockstore::ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX) rather than a dedicated `RecordNotCached` arm.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_block::constants::ZERO_HASH;
use dig_block::BlockStatus;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{
    BlockStore, BlockStoreError, CF_BLOCKS, CF_HEADERS, ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX,
};

use common::{temp_blockstore_dir, test_block, test_config};

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

fn read_cf_raw(path: &Path, cf: &str, key: &[u8]) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let h = db.cf_handle(cf)?;
    db.get_cf(h, key).ok().flatten()
}

/// **Proves:** AC §1 + §4 — after [`BlockStore::put_block`] seeds [`BlockStore::update_status`], [`BlockStore::get_record`]
/// returns the same structural record with the new status and matching [`dig_blockstore::BlockRecord::in_canonical_chain`]
/// ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) derives `in_canonical_chain` from [`BlockStatus::is_canonical`]).
#[test]
fn test_update_status_reflected_in_get_record() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(3, ZERO_HASH);
    let hash = block.hash();
    store.put_block(&block, false).expect("put");
    store
        .update_status(&hash, BlockStatus::SoftFinalized)
        .expect("update");
    let rec = store.get_record(&hash).expect("get").expect("some");
    assert_eq!(rec.status, BlockStatus::SoftFinalized);
    assert_eq!(
        rec.in_canonical_chain,
        BlockStatus::SoftFinalized.is_canonical()
    );
}

/// **Proves:** AC §2 — byte-identical snapshots of [`CF_HEADERS`] and [`CF_BLOCKS`] for the hash before vs after
/// [`update_status`] while the record cache mutates ([`BLK-010.md`](../docs/requirements/domains/block_storage/specs/BLK-010.md) AC §2).
#[test]
fn test_update_status_does_not_change_cf_headers_or_cf_blocks() {
    let (_guard, path) = temp_blockstore_dir();
    let path_buf = path.clone();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(4, ZERO_HASH);
    let hash = block.hash();
    store.put_block(&block, false).expect("put");
    let hk = hash_key(&hash);
    let headers_before = read_cf_raw(&path_buf, CF_HEADERS, hk.as_slice()).expect("headers");
    let blocks_before = read_cf_raw(&path_buf, CF_BLOCKS, hk.as_slice()).expect("blocks");
    store
        .update_status(&hash, BlockStatus::Orphaned)
        .expect("update");
    let headers_after = read_cf_raw(&path_buf, CF_HEADERS, hk.as_slice()).expect("headers");
    let blocks_after = read_cf_raw(&path_buf, CF_BLOCKS, hk.as_slice()).expect("blocks");
    assert_eq!(headers_before, headers_after);
    assert_eq!(blocks_before, blocks_after);
}

/// **Proves:** AC §3 — hash never inserted into the record cache yields [`BlockStoreError::Serialization`] with the
/// stable [`ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX`] prefix.
#[test]
fn test_update_status_unknown_hash_errors() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let orphan = test_block(99, ZERO_HASH).hash();
    let err = store
        .update_status(&orphan, BlockStatus::Validated)
        .expect_err("must error");
    match err {
        BlockStoreError::Serialization(s) => {
            assert!(
                s.starts_with(ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX),
                "unexpected message: {s}"
            );
        }
        e => panic!("unexpected {e:?}"),
    }
}

/// **Proves:** AC §3 — evicting the record with [`BlockStore::invalidate_record_cache_entry`] removes the cache row
/// without touching disk; [`update_status`] must fail until [`get_record`] repopulates the cache.
#[test]
fn test_update_status_after_invalidate_errors() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(6, ZERO_HASH);
    let hash = block.hash();
    store.put_block(&block, false).expect("put");
    store.invalidate_record_cache_entry(&hash);
    let err = store
        .update_status(&hash, BlockStatus::Rejected)
        .expect_err("no cache row");
    assert!(matches!(err, BlockStoreError::Serialization(_)));
    // Warm cache again via get_record, then update succeeds.
    assert!(store.get_record(&hash).expect("get").is_some());
    store
        .update_status(&hash, BlockStatus::Rejected)
        .expect("after warm");
    let rec = store.get_record(&hash).expect("get2").expect("some");
    assert_eq!(rec.status, BlockStatus::Rejected);
}

/// **Proves:** Test plan “multiple updates” — several [`update_status`] calls in a row; each [`get_record`] observes
/// the latest status ([`BLK-010.md`](../docs/requirements/domains/block_storage/specs/BLK-010.md) test plan).
#[test]
fn test_update_status_sequential_updates() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(8, ZERO_HASH);
    let hash = block.hash();
    store.put_block(&block, false).expect("put");
    for st in [
        BlockStatus::Pending,
        BlockStatus::Validated,
        BlockStatus::SoftFinalized,
    ] {
        store.update_status(&hash, st).expect("update");
        let rec = store.get_record(&hash).expect("get").expect("some");
        assert_eq!(rec.status, st);
        assert_eq!(rec.in_canonical_chain, st.is_canonical());
    }
}

/// **Sanity:** [`L2Block`] does not implement [`PartialEq`]; we still prove round-trip **identity** for the header
/// payload after status-only mutation — height and parent must match the original block ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md) stored header).
#[test]
fn test_update_status_preserves_header_derived_fields() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(12, ZERO_HASH);
    let hash = block.hash();
    store.put_block(&block, false).expect("put");
    store
        .update_status(&hash, BlockStatus::HardFinalized)
        .expect("upd");
    let rec = store.get_record(&hash).expect("get").expect("some");
    assert_eq!(rec.height, block.height());
    assert_eq!(rec.parent_hash, block.header.parent_hash);
}
