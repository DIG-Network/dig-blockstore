//! # CAC-003 — BlockRecord cache: in-memory only, derive from header on miss
//!
//! **Trace**
//! - Spec: [`CAC-003_block_record_cache.md`](../docs/requirements/domains/caching/specs/CAC-003_block_record_cache.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAC-003)](../docs/requirements/domains/caching/NORMATIVE.md)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/caching/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! The `BlockRecord` cache is an in-memory `HashMap` that is NEVER persisted to RocksDB.
//! On cache miss, records are derived from headers via `BlockRecord::from_header`. These
//! tests prove:
//!
//! 1. **Not persisted** — after close + reopen, the record cache is empty (records must
//!    be re-derived from headers on the next `get_record` call).
//! 2. **Derive on miss** — `get_record` for a stored block whose record is not cached
//!    derives it from the header and populates the cache.
//! 3. **put_block inserts** — `put_block` populates the record cache with `Validated` status.
//! 4. **update_status reflects** — `update_status` changes the cached record's status field.
//! 5. **set_canonical reflects** — `set_canonical` updates `in_canonical_chain` on the cached record.
//!
//! ## Chia analogy
//!
//! Chia persists `BlockRecord` in a SQLite table. DIG derives them on-the-fly from headers
//! to avoid write amplification — the record cache is purely an acceleration layer.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_block::BlockStatus;
use dig_blockstore::{BlockRecord, BlockStore};

use common::{temp_blockstore_dir, test_block, test_config};

#[test]
fn test_record_cache_put_block_inserts() {
    // **Proves:** CAC-003 AC §3 — put_block inserts a record with Validated status.
    //
    // **Requirement complete when:** After put_block, get_record returns a record
    // matching from_header(&header, Validated) without any RocksDB read.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    let rec = store
        .get_record(&b.hash())
        .expect("get_record")
        .expect("cached");
    let expected = BlockRecord::from_header(&b.header, BlockStatus::Validated);
    assert_eq!(rec, expected, "put_block must populate record cache");
    assert_eq!(
        store.cf_headers_physical_get_count(),
        0,
        "record served from cache, no CF_HEADERS read"
    );
}

#[test]
fn test_record_cache_hit_avoids_rocksdb() {
    // **Proves:** CAC-003 AC — cache hit serves records without touching RocksDB.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    // Multiple gets should all hit the cache
    for _ in 0..3 {
        store.get_record(&b.hash()).expect("get").expect("hit");
    }
    assert_eq!(store.cf_headers_physical_get_count(), 0);
}

#[test]
fn test_record_cache_miss_derives_from_header() {
    // **Proves:** CAC-003 AC §2 — cache miss derives record from header.
    //
    // **Requirement complete when:** After invalidating the record cache, get_record
    // still returns the correct record (derived from CF_HEADERS) and subsequent gets
    // are cache hits.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    // Invalidate record cache but leave header cache warm
    store.invalidate_record_cache_entry(&b.hash());

    let rec = store
        .get_record(&b.hash())
        .expect("derive on miss")
        .expect("some");
    assert_eq!(
        rec,
        BlockRecord::from_header(&b.header, BlockStatus::Validated)
    );
    // Header cache was warm, so no physical CF_HEADERS read needed
    assert_eq!(store.cf_headers_physical_get_count(), 0);
}

#[test]
fn test_record_cache_not_persisted() {
    // **Proves:** CAC-003 AC §1/§7 — record cache is empty after close + reopen.
    //
    // **Requirement complete when:** After reopening the store, get_record needs to
    // re-derive the record from CF_HEADERS (incrementing the physical read counter),
    // proving the record cache was not persisted.
    let (_guard, path) = temp_blockstore_dir();
    let b = test_block(0, Bytes32::default());
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        store.init_genesis(&b).expect("genesis");
        // Record cache is warm
        store.get_record(&b.hash()).expect("cached").expect("hit");
    }
    // Reopen — record cache is empty
    let store2 = BlockStore::open(test_config(path)).expect("reopen");
    assert_eq!(store2.cf_headers_physical_get_count(), 0);

    let rec = store2
        .get_record(&b.hash())
        .expect("derive on miss")
        .expect("some");
    // Record was re-derived from header, which required a CF_HEADERS read
    // (header cache is also empty after reopen since warm_cache_on_open=false in test_config)
    assert_eq!(store2.cf_headers_physical_get_count(), 1);
    assert_eq!(
        rec,
        BlockRecord::from_header(&b.header, BlockStatus::Validated)
    );
}

#[test]
fn test_record_cache_update_status_reflects() {
    // **Proves:** CAC-003 AC §4 — update_status modifies the cached record's status.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, Bytes32::default());
    store.init_genesis(&b).expect("genesis");

    // Initial status is Validated
    let rec1 = store.get_record(&b.hash()).expect("get").expect("rec");
    assert_eq!(rec1.status, BlockStatus::Validated);

    // Update to HardFinalized
    store
        .update_status(&b.hash(), BlockStatus::HardFinalized)
        .expect("update_status");

    let rec2 = store.get_record(&b.hash()).expect("get").expect("rec");
    assert_eq!(
        rec2.status,
        BlockStatus::HardFinalized,
        "status must reflect the update"
    );
}

#[test]
fn test_record_cache_set_canonical_reflects() {
    // **Proves:** CAC-003 AC §5 — set_canonical updates in_canonical_chain on the cached record.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = common::build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");

    // Store block at height 1 as NON-canonical
    store
        .put_block(&chain[1], false)
        .expect("put non-canonical");
    let _rec_before = store
        .get_record(&chain[1].hash())
        .expect("get")
        .expect("rec");
    // put_block uses Validated status; in_canonical_chain depends on the status
    // But the block was NOT added to CF_CANONICAL via put_block(canonical=false),
    // so set_canonical should update the record cache.

    store
        .set_canonical(&chain[1].hash())
        .expect("set_canonical");
    let rec_after = store
        .get_record(&chain[1].hash())
        .expect("get")
        .expect("rec");
    assert!(
        rec_after.in_canonical_chain,
        "set_canonical must update in_canonical_chain flag"
    );
}

#[test]
fn test_record_from_header_field_accuracy() {
    // **Proves:** CAC-003 AC §8 — derived BlockRecord matches header fields exactly.
    let header = common::test_header(42, Bytes32::new([7u8; 32]));
    let rec = BlockRecord::from_header(&header, BlockStatus::SoftFinalized);
    assert_eq!(rec.hash, header.hash());
    assert_eq!(rec.height, 42);
    assert_eq!(rec.parent_hash, Bytes32::new([7u8; 32]));
    assert_eq!(rec.status, BlockStatus::SoftFinalized);
    assert!(rec.in_canonical_chain); // SoftFinalized.is_canonical() == true
    assert_eq!(rec.block_size, 0, "from_header always sets block_size to 0");
}
