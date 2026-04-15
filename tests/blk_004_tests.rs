//! # BLK-004 — `get_record`: in-memory record cache, derive from header on miss
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-004.md`](../docs/requirements/domains/block_storage/specs/BLK-004.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-004)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-004-get-record-by-hash-get_record)
//! - Related types: [`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) (`BlockRecord` is not `Serialize`)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Proof strategy (maps to BLK-004 acceptance criteria)
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Record for every persisted header | [`test_get_record_matches_from_header_after_put_block`] |
//! | §2 | Record-cache hit ⇒ no `CF_HEADERS` physical `get_cf` | [`test_record_cache_hit_skips_physical_cf_headers_read`] |
//! | §3 | Miss derives via [`BlockRecord::from_header`], populates cache | [`test_record_cache_miss_with_warm_header_derives_without_rocksdb`], [`test_full_cache_miss_read_through_from_cf_headers`] |
//! | §4 | `BlockRecord` never stored in RocksDB | [`test_blockrecord_rebuilt_after_reopen_not_persisted`] + static note on [`BlockRecord`](dig_blockstore::BlockRecord) (no `serde::Serialize` in [`TYP-004`](typ_004_tests.rs)) |
//! | §5 | Unknown hash ⇒ `Ok(None)` | [`test_unknown_hash_returns_none`] |
//!
//! **Instrumentation:** [`BlockStore::cf_headers_physical_get_count`](dig_blockstore::BlockStore::cf_headers_physical_get_count) counts `get_cf` on [`CF_HEADERS`] from [`get_header`](dig_blockstore::BlockStore::get_header) and [`get_record`](dig_blockstore::BlockStore::get_record) when both the header LRU and record map miss (see store docs for `get_record` short-circuit when the header LRU is warm).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::{BlockStatus, L2BlockHeader};
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{BlockRecord, BlockStore, CF_BLOCKS, CF_HEADERS};

use common::{temp_blockstore_dir, test_block, test_config};

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Raw `CF_HEADERS` value — direct bincode of [`L2BlockHeader`] only ([`SER-002`](ser_002_tests.rs)); used to
/// prove we never stash a separate `BlockRecord` blob beside the header row.
fn read_cf_headers_raw(path: &Path, hash: &Bytes32) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_HEADERS)?;
    db.get_cf(cf, hash_key(hash).as_slice()).ok().flatten()
}

#[test]
fn test_get_record_matches_from_header_after_put_block() {
    // **Proves:** AC §1 — for any hash present in `CF_HEADERS`, [`get_record`](dig_blockstore::BlockStore::get_record)
    // returns the same logical summary as [`BlockRecord::from_header`] on that header ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md) normative snippet).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(7, ZERO_HASH);
    assert!(store.put_block(&b, false).expect("put"));
    let expected = BlockRecord::from_header(&b.header, BlockStatus::Validated);
    let got = store
        .get_record(&b.hash())
        .expect("get_record")
        .expect("some");
    assert_eq!(got, expected);
}

#[test]
fn test_record_cache_hit_skips_physical_cf_headers_read() {
    // **Proves:** AC §2 — after [`put_block`] seeds [`record_cache`](dig_blockstore::store::BlockStore), repeated
    // [`get_record`] calls must not increment [`cf_headers_physical_get_count`](dig_blockstore::BlockStore::cf_headers_physical_get_count)
    // (no RocksDB `get_cf` on [`CF_HEADERS`]).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(3, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_record(&b.hash()).expect("g1").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_record(&b.hash()).expect("g2").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
}

#[test]
fn test_record_cache_miss_with_warm_header_derives_without_rocksdb() {
    // **Proves:** AC §3 (derive path) — record-cache miss with header LRU still warm (typical after `put_block`):
    // [`get_record`] rebuilds [`BlockRecord`] from the in-memory [`L2BlockHeader`] without a physical `CF_HEADERS` read.
    //
    // **How:** [`invalidate_record_cache_entry`](dig_blockstore::BlockStore::invalidate_record_cache_entry) simulates
    // eviction of only the record map entry; header shard still holds the deserialized header.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(11, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    store.invalidate_record_cache_entry(&b.hash());
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    let got = store
        .get_record(&b.hash())
        .expect("get_record")
        .expect("some");
    assert_eq!(
        got,
        BlockRecord::from_header(&b.header, BlockStatus::Validated)
    );
    assert_eq!(
        store.cf_headers_physical_get_count(),
        0,
        "header warm path must not touch RocksDB CF_HEADERS"
    );
}

#[test]
fn test_full_cache_miss_read_through_from_cf_headers() {
    // **Proves:** AC §3 — when **both** caches are cold for this hash, [`get_record`] performs exactly one physical
    // `get_cf` on [`CF_HEADERS`], deserializes [`L2BlockHeader`], seeds both caches, and returns
    // [`BlockRecord::from_header`]. A second call hits the record cache and adds no further physical reads.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(5, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    store.invalidate_record_cache_entry(&b.hash());
    store.invalidate_header_cache_entry(&b.hash());
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    let r1 = store.get_record(&b.hash()).expect("first").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);
    let r2 = store.get_record(&b.hash()).expect("second").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);
    assert_eq!(r1, r2);
    assert_eq!(
        r1,
        BlockRecord::from_header(&b.header, BlockStatus::Validated)
    );
}

#[test]
fn test_blockrecord_rebuilt_after_reopen_not_persisted() {
    // **Proves:** AC §4 — [`BlockRecord`] is not a durable column: close the store (drop in-memory `record_cache`),
    // reopen on the same path, and verify [`get_record`] still succeeds by **re-deriving** from the persisted
    // [`CF_HEADERS`] row ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md): no serde persist path).
    //
    // **Contrast:** If `BlockRecord` were written to disk, we would expect either a dedicated CF or duplicate bytes;
    // the schema exposes only block bodies + headers ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
    let (_guard, path) = temp_blockstore_dir();
    let b = test_block(9, ZERO_HASH);
    {
        let store = BlockStore::open(test_config(path.clone())).expect("open");
        store.put_block(&b, false).expect("put");
    }
    let store2 = BlockStore::open(test_config(path)).expect("reopen");
    assert_eq!(
        store2
            .get_record(&b.hash())
            .expect("get_record")
            .expect("some"),
        BlockRecord::from_header(&b.header, BlockStatus::Validated),
        "record must be reconstructed from header bytes, not loaded from a BlockRecord row"
    );
}

#[test]
fn test_cf_headers_row_is_header_bincode_only() {
    // **Proves:** AC §4 (layout) — the only user payload under [`CF_HEADERS`] for this hash is bincode(`L2BlockHeader`).
    // There is no parallel `BlockRecord` encoding: comparing raw bytes to [`bincode::serialize`] of the header matches
    // [`read_cf_headers_raw`], and [`BlockRecord`] remains non-`Serialize` at compile time ([`block_record.rs`](../src/types/block_record.rs)).
    let (_guard, path) = temp_blockstore_dir();
    let path_buf = path.clone();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(13, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let raw = read_cf_headers_raw(path_buf.as_path(), &b.hash()).expect("header row");
    let decoded: L2BlockHeader = bincode::deserialize(&raw).expect("bincode header");
    assert_eq!(decoded, b.header);
    let expected_bytes = bincode::serialize(&b.header).expect("header serde");
    assert_eq!(raw, expected_bytes);
}

#[test]
fn test_unknown_hash_returns_none() {
    // **Proves:** AC §5 — missing header ⇒ [`get_record`] returns `Ok(None)` ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md)).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let unknown = Bytes32::new([0xAB; 32]);
    assert!(store.get_record(&unknown).expect("get").is_none());
}

#[test]
fn test_cf_blocks_still_zstd_not_blockrecord() {
    // **Proves:** AC §4 — [`CF_BLOCKS`] holds compressed block bodies ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)), not `BlockRecord` structs.
    //
    // **Signal:** zstd magic prefix distinguishes block blobs from header bincode; `BlockRecord` never appears as a CF value type.
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
    let (_guard, path) = temp_blockstore_dir();
    let path_buf = path.clone();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(4, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db =
        rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path_buf.as_path(), cfs, false)
            .expect("ro");
    let cf = db.cf_handle(CF_BLOCKS).expect("cf blocks");
    let raw = db
        .get_cf(cf, hash_key(&b.hash()).as_slice())
        .expect("get")
        .expect("blob");
    assert!(
        raw.len() >= 4 && raw[0..4] == ZSTD_MAGIC,
        "CF_BLOCKS must remain zstd-framed block payload"
    );
}
