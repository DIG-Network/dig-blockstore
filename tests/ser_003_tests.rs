//! # SER-003 — Wire-format interop: **`chia-traits::Streamable`** for [`dig_block::L2Block`]
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`SER-003.md`](../docs/requirements/domains/serialization/specs/SER-003.md)
//! - NORMATIVE: [`NORMATIVE.md` (SER-003)](../docs/requirements/domains/serialization/NORMATIVE.md#ser-003-wire-format-interop)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/serialization/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Public API:** [`block_to_wire_bytes`] / [`block_from_wire_bytes`] ([`dig_blockstore`]) adapt [`Streamable`] to [`BlockStoreError::Serialization`]
//!   as required by the error taxonomy for malformed payloads ([`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002.md)).
//! - **Encoding:** [`dig_block::L2Block`] implements [`Streamable`] in **dig-block** (derive); these tests prove the whole block round-trips
//!   through the blockstore façade, matching the spec’s “Chia wire” contract—not bincode ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
//! - **Identity:** [`L2Block`] has no [`PartialEq`]; equality uses [`L2Block::hash`] (same convention as [`SER-001`](ser_001_tests.rs)).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::io::Cursor;

use chia_protocol::Bytes32;
use chia_traits::Streamable;
use dig_block::L2Block;
use dig_blockstore::{block_from_wire_bytes, block_to_wire_bytes, BlockStore, BlockStoreError};

use common::{test_block, test_config};

/// Zstd frame magic (little-endian `0xFD2FB528`) — wire payloads MUST NOT be zstd-wrapped ([`SER-003`] AC §6).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

fn assert_same_block(a: &L2Block, b: &L2Block) {
    assert_eq!(
        a.hash(),
        b.hash(),
        "block hash must match after Streamable round-trip (canonical identity)"
    );
}

#[test]
fn test_wire_round_trip_preserves_block_hash() {
    // **Proves:** SER-003 test plan “round-trip” + AC §2 — `block_from_wire_bytes(block_to_wire_bytes(b))` preserves identity.
    let block = test_block(2, Bytes32::default());
    let wire = block_to_wire_bytes(&block).expect("block_to_wire_bytes");
    let back = block_from_wire_bytes(&wire).expect("block_from_wire_bytes");
    assert_same_block(&block, &back);
}

#[test]
fn test_wire_bytes_differ_from_bincode_storage_encoding() {
    // **Proves:** AC §3 — same logical block encodes differently on wire vs storage (bincode layout/endianness ≠ Streamable).
    let block = test_block(1, Bytes32::default());
    let wire = block_to_wire_bytes(&block).expect("wire");
    let storage = bincode::serialize(&block).expect("bincode storage");
    assert_ne!(
        wire, storage,
        "Streamable wire bytes must not equal bincode-serialized bytes for the same block"
    );
}

#[test]
fn test_raw_streamable_parse_matches_facade() {
    // **Proves:** SER-003 test plan “Streamable compatibility” — [`L2Block::parse`] agrees with [`block_from_wire_bytes`].
    let block = test_block(0, Bytes32::default());
    let wire = block_to_wire_bytes(&block).expect("wire");
    let mut cursor = Cursor::new(wire.as_slice());
    let direct = L2Block::parse::<false>(&mut cursor).expect("L2Block::parse");
    assert_eq!(
        cursor.position(),
        wire.len() as u64,
        "parse must consume entire frame (no trailing slack)"
    );
    assert_same_block(&block, &direct);
    let via_facade = block_from_wire_bytes(&wire).expect("facade");
    assert_same_block(&direct, &via_facade);
}

#[test]
fn test_invalid_wire_bytes_yield_serialization_error() {
    // **Proves:** AC §5 — garbage / truncated input surfaces as [`BlockStoreError::Serialization`].
    let junk = [0xFFu8; 4];
    let err = block_from_wire_bytes(&junk).expect_err("random bytes should fail");
    assert!(
        matches!(err, BlockStoreError::Serialization(_)),
        "expected Serialization, got {err:?}"
    );

    let block = test_block(0, Bytes32::default());
    let wire = block_to_wire_bytes(&block).expect("wire");
    assert!(wire.len() > 3, "precondition for truncation test");
    let truncated = &wire[..wire.len() - 3];
    let err2 = block_from_wire_bytes(truncated).expect_err("truncated");
    assert!(matches!(err2, BlockStoreError::Serialization(_)));
}

#[test]
fn test_wire_payload_has_no_zstd_framing() {
    // **Proves:** SER-003 test plan “no compression” / AC §6 — first bytes are not a zstd frame magic.
    let block = test_block(3, Bytes32::default());
    let wire = block_to_wire_bytes(&block).expect("wire");
    assert!(
        wire.len() >= 4,
        "non-trivial block should yield non-empty wire encoding"
    );
    assert_ne!(
        wire[..4],
        ZSTD_MAGIC,
        "wire format must not be zstd-compressed at this layer"
    );
}

#[test]
fn test_wire_encoding_is_deterministic() {
    // **Proves:** SER-003 test plan “deterministic” — identical block → identical bytes (stable for golden vectors / caches).
    let block = test_block(5, Bytes32::default());
    let a = block_to_wire_bytes(&block).expect("first");
    let b = block_to_wire_bytes(&block).expect("second");
    assert_eq!(a, b);
}

#[test]
fn test_trailing_bytes_rejected_by_facade() {
    // **Proves:** [`Streamable::from_bytes`] contract — extra bytes after a valid block are an error (peer framing bugs).
    let block = test_block(1, Bytes32::default());
    let mut wire = block_to_wire_bytes(&block).expect("wire");
    wire.extend_from_slice(&[0u8, 1, 2, 3]);
    let err = block_from_wire_bytes(&wire).expect_err("trailing garbage");
    assert!(matches!(err, BlockStoreError::Serialization(_)));
}

#[test]
fn test_blockstore_dependency_smoke_includes_streamable_block() {
    // **Proves:** Integration sanity — a [`BlockStore`] constructed in the normal path still compiles alongside wire helpers;
    // SER-003 is orthogonal to RocksDB but must not break the primary handle ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    let block = test_block(0, Bytes32::default());
    let wire = block_to_wire_bytes(&block).unwrap();
    let _ = block_from_wire_bytes(&wire).unwrap();

    let (dir, path) = common::temp_blockstore_dir();
    let cfg = test_config(path);
    let store = BlockStore::open(cfg).expect("open");
    assert!(store.tip().is_none());
    drop(store);
    drop(dir);
}
