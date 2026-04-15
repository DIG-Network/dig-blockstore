//! # SER-002 — Header serialization: **bincode only** (no compression)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`SER-002.md`](../docs/requirements/domains/serialization/specs/SER-002.md)
//! - NORMATIVE: [`NORMATIVE.md` (SER-002)](../docs/requirements/domains/serialization/NORMATIVE.md#ser-002-header-serialization)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/serialization/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **API:** [`BlockStore::serialize_header`] / [`BlockStore::deserialize_header`] implement the normative
//!   `L2BlockHeader` → `CF_HEADERS` path without zstd ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
//! - **Production wiring:** [`BlockStore::init_genesis`] writes header bytes via [`serialize_header`](dig_blockstore::BlockStore::serialize_header),
//!   so persisted values match the public serializer ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
//! - **Raw bincode:** One test deserializes with [`bincode::deserialize`] directly to prove no hidden compression
//!   wrapper — any future “accidental zstd” would break that contract.
//! - **Identity:** [`L2BlockHeader`] implements [`PartialEq`]; equality after round-trip proves all fields survive
//!   the storage codec (unlike [`L2Block`], which uses hash-only identity in other tests).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_block::L2BlockHeader;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{BlockStore, BlockStoreError, CF_HEADERS};

use common::{temp_blockstore_dir, test_block, test_config, test_header};

/// ZSTD framed block magic (little-endian `0xFD2FB528`) — MUST NOT prefix header payloads ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md) AC §4).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Reads raw `CF_HEADERS` value for `hash` after all [`BlockStore`] handles are dropped (direct RocksDB verify).
///
/// **Proves:** Acceptance §3 — bytes on disk are exactly what [`bincode::deserialize`] expects, not a compressed blob.
fn read_headers_raw(path: &Path, hash: &Bytes32) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_HEADERS)?;
    db.get_cf(cf, hash_key(hash).as_slice()).ok().flatten()
}

#[test]
fn test_round_trip_header_fields_match() {
    // **Proves:** SER-002 test plan “round-trip” + AC §2 — `deserialize_header(serialize_header(h))` preserves every field.
    let header = test_header(7, ZERO_HASH);
    let bytes = BlockStore::serialize_header(&header).expect("serialize_header");
    let back = BlockStore::deserialize_header(&bytes).expect("deserialize_header");
    assert_eq!(back, header, "PartialEq on L2BlockHeader must hold after bincode round-trip");
}

#[test]
fn test_no_zstd_framing_prefix() {
    // **Proves:** AC §4 — first four bytes MUST NOT be the zstd magic (headers are never zstd-wrapped).
    let header = test_header(0, ZERO_HASH);
    let bytes = BlockStore::serialize_header(&header).expect("serialize_header");
    assert!(
        bytes.len() >= 4,
        "serialized header should have non-trivial length for magic check"
    );
    assert_ne!(
        bytes[..4],
        ZSTD_MAGIC[..],
        "CF_HEADERS payload must not begin with zstd frame magic"
    );
}

#[test]
fn test_raw_bincode_decode_without_blockstore_helper() {
    // **Proves:** AC §3 — external `bincode::deserialize` on stored bytes succeeds (no decompression prelude).
    let header = test_header(3, Bytes32::default());
    let stored = BlockStore::serialize_header(&header).expect("serialize_header");
    let decoded: L2BlockHeader =
        bincode::deserialize(&stored).expect("direct bincode::deserialize must succeed on CF_HEADERS bytes");
    assert_eq!(decoded, header);
}

#[test]
fn test_serialized_size_bounded_no_anomalous_expansion() {
    // **Proves:** SER-002 test plan “size proportionality” — lengths stay in a tight band for varied content
    // (no compression dictionary blobs or multi-megabyte expansion from a ~700 B preimage type).
    //
    // **Bounds:** [`L2BlockHeader::HASH_PREIMAGE_LEN`] is 710 in dig-block; bincode adds serde structure overhead
    // but must remain far below block-scale payloads. We use a generous window so minor dig-block layout churn
    // does not flake CI while still catching accidental embedding of auxiliary buffers.
    let block_at_1 = test_block(1, ZERO_HASH);
    let parents = [ZERO_HASH, Bytes32::default(), block_at_1.hash()];
    for (i, parent) in parents.iter().enumerate() {
        let h = test_header(i as u64 * 100, *parent);
        let n = BlockStore::serialize_header(&h)
            .expect("serialize_header")
            .len();
        assert!(
            (200..=900).contains(&n),
            "unexpected header serialization len {n} for scenario {i} — check for accidental extra framing"
        );
    }
}

#[test]
fn test_truncated_bytes_yield_serialization_error() {
    // **Proves:** SER-002 test plan “corrupted input” — malformed bincode maps to [`BlockStoreError::Serialization`].
    let header = test_header(0, ZERO_HASH);
    let full = BlockStore::serialize_header(&header).expect("serialize_header");
    assert!(full.len() > 8, "precondition: need room to truncate meaningfully");
    let truncated = &full[..full.len() - 8];
    let err = BlockStore::deserialize_header(truncated).expect_err("truncated bincode should fail");
    assert!(
        matches!(err, BlockStoreError::Serialization(_)),
        "expected Serialization variant, got {err:?}"
    );
}

#[test]
fn test_init_genesis_cf_headers_matches_serialize_header() {
    // **Proves:** Integration — [`BlockStore::init_genesis`] persists the same bytes as [`BlockStore::serialize_header`]
    // for the genesis header, so on-disk layout matches the public API ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md) AC §1–§3).
    let (dir, path) = temp_blockstore_dir();
    let block = test_block(0, ZERO_HASH);
    let hash = block.hash();
    let expected = BlockStore::serialize_header(&block.header).expect("serialize_header");

    {
        let cfg = test_config(path.clone());
        let store = BlockStore::open(cfg).expect("open");
        store.init_genesis(&block).expect("init_genesis");
    }

    let raw = read_headers_raw(dir.path(), &hash).expect("header value should exist after genesis");
    assert_eq!(
        raw, expected,
        "CF_HEADERS bytes must equal serialize_header(header) exactly"
    );
    let via_api = BlockStore::deserialize_header(&raw).expect("deserialize_header");
    assert_eq!(via_api, block.header);

    // Dropping `dir` cleans up temp RocksDB.
    drop(dir);
}

#[test]
fn test_empty_slice_deserialization_fails_as_serialization() {
    // **Proves:** Robustness edge — zero-length input cannot be a valid header; still classified as Serialization
    // (same bucket as truncations) for caller consistency.
    let err = BlockStore::deserialize_header(&[]).expect_err("empty slice");
    assert!(matches!(err, BlockStoreError::Serialization(_)));
}

#[test]
fn test_distinct_headers_distinct_bytes() {
    // **Proves:** Deterministic bincode — different logical headers produce different payloads (no accidental
    // constant serialization), supporting hash-keyed `CF_HEADERS` lookups later ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md) precursor).
    let a = test_header(0, ZERO_HASH);
    let b = test_header(1, ZERO_HASH);
    let ba = BlockStore::serialize_header(&a).unwrap();
    let bb = BlockStore::serialize_header(&b).unwrap();
    assert_ne!(ba, bb, "different headers must not serialize identically");
}
