//! # BLK-001 — Store block (`put` / `put_block`): bodies, headers, record cache, idempotency
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-001.md`](../docs/requirements/domains/block_storage/specs/BLK-001.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-001)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-001-store-block-put)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Proof strategy (maps to BLK-001 acceptance criteria)
//!
//! | AC | Test idea |
//! |----|-----------|
//! | §1 New insert returns `Ok(true)` | [`test_put_block_inserts_returns_true`] |
//! | §2 Duplicate hash returns `Ok(false)` without clobbering bytes | [`test_put_block_idempotent_no_overwrite`] |
//! | §3 `CF_BLOCKS` holds zstd-framed payload | [`test_cf_blocks_payload_is_zstd_frame`] |
//! | §4 `CF_HEADERS` holds raw bincode (no zstd magic) | [`test_cf_headers_is_uncompressed_bincode`] |
//! | §5 `BlockRecord` cache populated; [`get_record`] matches [`BlockRecord::from_header`] | [`test_get_record_matches_expected_after_put_block`] |
//! | §6 `canonical=true` ⇒ height key in `CF_CANONICAL` | [`test_canonical_true_writes_height_index`] |
//! | §7 `canonical=false` ⇒ no row at that height | [`test_canonical_false_skips_canonical_cf`] |
//!
//! **Round-trip identity:** [`L2Block`] has no [`PartialEq`]; we assert [`L2Block::hash`] equality after
//! [`get_block`](dig_blockstore::BlockStore::get_block) ([`SER-004`](../docs/requirements/domains/serialization/specs/SER-004.md) precedent).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::BlockStatus;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::{hash_key, height_key};
use dig_blockstore::{BlockRecord, BlockStore, CF_BLOCKS, CF_CANONICAL, CF_HEADERS};

use common::{temp_blockstore_dir, test_block, test_config};

/// ZSTD framed block magic (little-endian `0xFD2FB528`) — [`CF_BLOCKS`] values are zstd-compressed ([`SER-001`](ser_001_tests.rs)).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Raw `CF_BLOCKS` value for `hash` (direct RocksDB) — proves persistence layout independent of [`BlockStore`] helpers.
fn read_cf_blocks_raw(path: &Path, hash: &Bytes32) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_BLOCKS)?;
    db.get_cf(cf, hash_key(hash).as_slice()).ok().flatten()
}

/// Raw `CF_HEADERS` value — must be direct bincode ([`SER-002`](ser_002_tests.rs)).
fn read_cf_headers_raw(path: &Path, hash: &Bytes32) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_HEADERS)?;
    db.get_cf(cf, hash_key(hash).as_slice()).ok().flatten()
}

/// Raw `CF_CANONICAL` value at `height` (big-endian u64 key).
fn read_cf_canonical_at_height(path: &Path, height: u64) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_CANONICAL)?;
    db.get_cf(cf, height_key(height)).ok().flatten()
}

#[test]
fn test_put_block_inserts_returns_true() {
    // **Proves:** AC §1 — first store must report `true` so callers can distinguish fresh inserts from idempotent no-ops.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(1, ZERO_HASH);
    assert!(
        store.put_block(&b, true).expect("put_block"),
        "novel hash must return Ok(true)"
    );
    let got = store
        .get_block(&b.hash())
        .expect("get_block")
        .expect("present");
    assert_eq!(
        got.hash(),
        b.hash(),
        "BLK-001 test plan: round-trip via get_block"
    );
}

#[test]
fn test_put_block_idempotent_no_overwrite() {
    // **Proves:** AC §2 — second `put_block` with same hash returns `Ok(false)`; bytes in CF_BLOCKS stay unchanged.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(2, ZERO_HASH);
    assert!(store.put_block(&b, false).expect("first"));
    let raw_after_first = read_cf_blocks_raw(path.as_path(), &b.hash()).expect("blob");
    assert!(
        !store.put_block(&b, true).expect("second call"),
        "duplicate must return false"
    );
    let raw_after_second = read_cf_blocks_raw(path.as_path(), &b.hash()).expect("blob");
    assert_eq!(
        raw_after_first, raw_after_second,
        "idempotent put must not rewrite CF_BLOCKS"
    );
}

#[test]
fn test_cf_blocks_payload_is_zstd_frame() {
    // **Proves:** AC §3 — block bodies are zstd-compressed full blocks ([`BlockStore::serialize_block`] pipeline).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(3, ZERO_HASH);
    store.put_block(&b, false).expect("put_block");
    let raw = read_cf_blocks_raw(path.as_path(), &b.hash()).expect("cf_blocks");
    assert!(
        raw.len() >= 4 && raw[..4] == ZSTD_MAGIC[..],
        "CF_BLOCKS must contain a zstd frame (magic prefix)"
    );
}

#[test]
fn test_cf_headers_is_uncompressed_bincode() {
    // **Proves:** AC §4 — header path never wraps zstd; raw bytes deserialize with [`BlockStore::deserialize_header`].
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(4, ZERO_HASH);
    store.put_block(&b, false).expect("put_block");
    let raw = read_cf_headers_raw(path.as_path(), &b.hash()).expect("cf_headers");
    assert_ne!(
        raw.get(..4).unwrap_or(&[]),
        &ZSTD_MAGIC[..],
        "CF_HEADERS must not start with zstd magic"
    );
    let hdr = BlockStore::deserialize_header(&raw).expect("bincode header");
    assert_eq!(hdr.hash(), b.header.hash());
}

#[test]
fn test_get_record_matches_expected_after_put_block() {
    // **Proves:** AC §5 — in-memory record cache matches [`BlockRecord::from_header`] with storage default status
    // ([`BlockStatus::Validated`] — structurally valid persisted block).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(5, ZERO_HASH);
    store.put_block(&b, true).expect("put_block");
    let expected = BlockRecord::from_header(&b.header, BlockStatus::Validated);
    let got = store
        .get_record(&b.hash())
        .expect("get_record")
        .expect("cached");
    assert_eq!(got, expected);
}

#[test]
fn test_canonical_true_writes_height_index() {
    // **Proves:** AC §6 — `canonical=true` installs `height_key(height) -> hash` in [`CF_CANONICAL`].
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(6, ZERO_HASH);
    store.put_block(&b, true).expect("put_block");
    let raw = read_cf_canonical_at_height(path.as_path(), 6).expect("canonical row");
    assert_eq!(raw.as_slice(), b.hash().as_ref());
}

#[test]
fn test_canonical_false_skips_canonical_cf() {
    // **Proves:** AC §7 — `canonical=false` must not create a height mapping for this block.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(7, ZERO_HASH);
    store.put_block(&b, false).expect("put_block");
    assert!(
        read_cf_canonical_at_height(path.as_path(), 7).is_none(),
        "non-canonical put must not touch CF_CANONICAL at this height"
    );
}
