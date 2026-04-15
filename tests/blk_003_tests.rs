//! # BLK-003 — `get_header`: header LRU first, then `CF_HEADERS` + bincode (no zstd)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-003.md`](../docs/requirements/domains/block_storage/specs/BLK-003.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-003)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-003-get-header-by-hash-get_header)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//! - Header cache shape: [`CAC-002`](../docs/requirements/domains/caching/specs/CAC-002_sharded_header_cache.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Header from `put_block` round-trips | [`test_get_header_matches_put_block`] |
//! | §2 | Cache hit ⇒ no `CF_HEADERS` `get_cf` | [`test_header_cache_hit_skips_physical_read`] |
//! | §3 | Miss uses bincode only (no zstd framing) | [`test_cf_headers_raw_is_direct_bincode`] |
//! | §4 | Read-through repopulates LRU | [`test_header_cache_miss_read_through_then_hit`] |
//! | §5 | Unknown hash ⇒ `None` | [`test_unknown_header_hash_returns_none`] |
//!
//! **Instrumentation:** [`BlockStore::cf_headers_physical_get_count`](dig_blockstore::BlockStore::cf_headers_physical_get_count).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::L2BlockHeader;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{BlockStore, CF_HEADERS};

use common::{temp_blockstore_dir, test_block, test_config};

/// ZSTD framed magic — MUST NOT prefix [`CF_HEADERS`] payloads ([`SER-002`](ser_002_tests.rs)).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

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
fn test_get_header_matches_put_block() {
    // **Proves:** AC §1 — [`get_header`](dig_blockstore::BlockStore::get_header) returns the same logical header as was
    // embedded in the stored block ([`L2BlockHeader`] is [`PartialEq`]).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(12, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = store.get_header(&b.hash()).expect("get_header").expect("some");
    assert_eq!(h, b.header);
}

#[test]
fn test_header_cache_hit_skips_physical_read() {
    // **Proves:** AC §2 — after [`put_block`] seeds [`ShardedHeaderCache`](dig_blockstore::cache::sharded::ShardedHeaderCache),
    // repeated [`get_header`] calls must not increment [`cf_headers_physical_get_count`](dig_blockstore::BlockStore::cf_headers_physical_get_count).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(2, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_header(&b.hash()).expect("g1").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_header(&b.hash()).expect("g2").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 0);
}

#[test]
fn test_header_cache_miss_read_through_then_hit() {
    // **Proves:** AC §4 — [`invalidate_header_cache_entry`](dig_blockstore::BlockStore::invalidate_header_cache_entry)
    // forces one physical read; the next [`get_header`] is served from LRU (counter stable).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(3, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let hash = b.hash();
    store.invalidate_header_cache_entry(&hash);
    assert_eq!(store.cf_headers_physical_get_count(), 0);
    store.get_header(&hash).expect("miss").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);
    store.get_header(&hash).expect("hit").expect("some");
    assert_eq!(store.cf_headers_physical_get_count(), 1);
}

#[test]
fn test_cf_headers_raw_is_direct_bincode() {
    // **Proves:** AC §3 — on-disk bytes are raw bincode ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md));
    // no zstd wrapper, so [`bincode::deserialize`] works directly (same contract as [`BLK-001`](blk_001_tests.rs) header test).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(4, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let raw = read_cf_headers_raw(path.as_path(), &b.hash()).expect("cf_headers");
    assert_ne!(
        raw.get(..4).unwrap_or(&[]),
        &ZSTD_MAGIC[..],
        "headers must not be zstd-framed"
    );
    let hdr: L2BlockHeader = bincode::deserialize(&raw).expect("direct bincode");
    assert_eq!(hdr, b.header);
}

#[test]
fn test_unknown_header_hash_returns_none() {
    // **Proves:** AC §5 — missing rows return `Ok(None)`; probe still counts as a physical read (observability).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let ghost = Bytes32::new([9u8; 32]);
    assert!(store.get_header(&ghost).expect("get").is_none());
    assert_eq!(store.cf_headers_physical_get_count(), 1);
}
