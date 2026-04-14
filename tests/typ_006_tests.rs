//! # TYP-006 — [`dig_blockstore::ChainTip`] 40-byte wire encoding
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-006.md`](../docs/requirements/domains/storage_types/specs/TYP-006.md)
//! - NORMATIVE: [`NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md#typ-006-chaintip-struct)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Layout:** [`ChainTip::to_bytes`](dig_blockstore::ChainTip::to_bytes) must place the raw
//!   [`chia_protocol::Bytes32`] in `bytes[0..32]` and [`u64::to_le_bytes`] in `bytes[32..40]`
//!   ([`TYP-006`](../docs/requirements/domains/storage_types/specs/TYP-006.md) encoding table).
//! - **Round-trip:** For arbitrary tips, [`ChainTip::from_bytes`](dig_blockstore::ChainTip::from_bytes)
//!   inverts `to_bytes`, proving the codec is lossless on the domain of valid values.
//! - **Length gate:** Any slice whose length is not exactly 40 must fail decode. The crate’s
//!   [`dig_blockstore::BlockStoreError`] surface is capped by [`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstore_error_enum.md);
//!   malformed fixed-width tip bytes therefore map to [`BlockStoreError::Serialization`](dig_blockstore::BlockStoreError::Serialization)
//!   with a stable message prefix (same idea as the private `load_tip` path in `src/store.rs` when `META_TIP` is corrupt).
//! - **Known vector:** A fully specified hash (`0xFF..`) and height (`42`) yields a single deterministic
//!   **40-byte** array, catching accidental endian or offset mistakes.
//! - **Copy / Eq:** [`ChainTip`] is `Copy` + `Eq`; assigning copies the value so both bindings remain valid,
//!   which matters for hot-path tip reads without heap allocation ([`TYP-006` design notes](../docs/requirements/domains/storage_types/specs/TYP-006.md)).

#![forbid(unsafe_code)]

use chia_protocol::Bytes32;
use dig_blockstore::{BlockStoreError, ChainTip};

/// Asserts `from_bytes` rejects `bytes` with [`BlockStoreError::Serialization`] and the TYP-006 message shape.
///
/// **Rationale:** ERR-001 does not define `InvalidData`; length violations for META_TIP are serialization-shaped
/// errors (fixed-width parse failure), consistent with [`dig_blockstore::ChainTip::from_bytes`].
fn assert_from_bytes_wrong_length(bytes: &[u8], expected_len_in_message: usize) {
    let err = ChainTip::from_bytes(bytes).expect_err("non-40-byte input must not decode");
    match err {
        BlockStoreError::Serialization(msg) => {
            assert!(
                msg.contains("ChainTip requires 40 bytes"),
                "message should describe required width: {msg}"
            );
            assert!(
                msg.contains(&format!("got {expected_len_in_message}")),
                "message should report actual length (expected {expected_len_in_message} in msg): {msg}"
            );
        }
        other => panic!("expected Serialization wrong-length error, got {other:?}"),
    }
}

#[test]
fn test_to_bytes_length() {
    // **What:** `to_bytes` always returns a fixed-width array.
    // **Proves:** TYP-006 acceptance — “exactly 40 bytes” without relying on `from_bytes`.
    let tip = ChainTip {
        hash: Bytes32::new([0u8; 32]),
        height: 0,
    };
    assert_eq!(tip.to_bytes().len(), 40);
}

#[test]
fn test_to_bytes_hash_position() {
    // **What:** The first 32 bytes equal the raw hash bytes.
    // **Proves:** TYP-006 `bytes[0..32] = hash` (raw Bytes32 as stored).
    let hash = Bytes32::new([0x11; 32]);
    let tip = ChainTip {
        hash,
        height: 0x1234_5678_9ABC_DEF0_u64,
    };
    let out = tip.to_bytes();
    assert_eq!(&out[0..32], hash.as_ref());
}

#[test]
fn test_to_bytes_height_position() {
    // **What:** The trailing 8 bytes are little-endian `height`.
    // **Proves:** TYP-006 `bytes[32..40] = height` LE (matches RocksDB META_TIP layout in SPEC3.4).
    let tip = ChainTip {
        hash: Bytes32::new([0u8; 32]),
        height: 0x0102_0304_0506_0708,
    };
    let out = tip.to_bytes();
    assert_eq!(&out[32..40], &tip.height.to_le_bytes());
}

#[test]
fn test_from_bytes_roundtrip() {
    // **What:** encode then decode recovers the original struct.
    // **Proves:** TYP-006 round-trip criterion for a non-trivial tip.
    let tip = ChainTip {
        hash: Bytes32::new([7u8; 32]),
        height: 99_999,
    };
    let bytes = tip.to_bytes();
    let back = ChainTip::from_bytes(&bytes).expect("round-trip must succeed");
    assert_eq!(back, tip);
}

#[test]
fn test_from_bytes_wrong_length() {
    // **What:** 39 bytes cannot be a valid tip payload.
    // **Proves:** TYP-006 test plan `test_from_bytes_wrong_length`.
    let mut b = [0u8; 39];
    b[0..32].fill(0xAA);
    assert_from_bytes_wrong_length(&b, 39);
}

#[test]
fn test_from_bytes_empty() {
    // **What:** Empty slice is rejected.
    // **Proves:** TYP-006 `test_from_bytes_empty` — boundary at minimum length.
    assert_from_bytes_wrong_length(&[], 0);
}

#[test]
fn test_from_bytes_too_long() {
    // **What:** 41 bytes are rejected (no implicit truncation).
    // **Proves:** TYP-006 `test_from_bytes_too_long` — defensive parse for future-extended values.
    let mut b = [0u8; 41];
    b.fill(0x55);
    assert_from_bytes_wrong_length(&b, 41);
}

#[test]
fn test_known_encoding() {
    // **What:** Fully specified hash (all `0xFF`) and height `42` produce one exact 40-byte blob.
    // **Proves:** TYP-006 known-value test; catches off-by-one in slice ranges or endianness.
    let tip = ChainTip {
        hash: Bytes32::new([0xFF; 32]),
        height: 42,
    };
    let mut expected = [0u8; 40];
    expected[0..32].fill(0xFF);
    expected[32..40].copy_from_slice(&42u64.to_le_bytes());
    assert_eq!(tip.to_bytes(), expected);
    assert_eq!(
        ChainTip::from_bytes(&expected).expect("known vector decodes"),
        tip
    );
}

#[test]
fn test_height_zero() {
    // **What:** Height `0` round-trips (genesis-adjacent tip).
    // **Proves:** TYP-006 `test_height_zero` — LE encoding of zero is eight zero bytes.
    let tip = ChainTip {
        hash: Bytes32::new([3u8; 32]),
        height: 0,
    };
    let back = ChainTip::from_bytes(&tip.to_bytes()).expect("height zero must round-trip");
    assert_eq!(back, tip);
    assert_eq!(&tip.to_bytes()[32..40], &[0u8; 8]);
}

#[test]
fn test_height_max() {
    // **What:** `u64::MAX` round-trips through LE bytes.
    // **Proves:** TYP-006 `test_height_max` — all height bits participate in encoding.
    let tip = ChainTip {
        hash: Bytes32::new([0xEE; 32]),
        height: u64::MAX,
    };
    let back = ChainTip::from_bytes(&tip.to_bytes()).expect("u64::MAX must round-trip");
    assert_eq!(back, tip);
}

#[test]
fn test_copy_semantics() {
    // **What:** `ChainTip` is `Copy`; binding duplication does not invalidate the original.
    // **Proves:** TYP-006 `test_copy_semantics` + NORMATIVE design note (small POD tip).
    let tip = ChainTip {
        hash: Bytes32::new([0xC0; 32]),
        height: 12345,
    };
    let copy = tip;
    assert_eq!(tip, copy);
    assert_eq!(tip.height, 12345);
    assert_eq!(copy.height, 12345);
    // Both should independently round-trip (copy is not a stale reference).
    assert_eq!(
        ChainTip::from_bytes(&tip.to_bytes()).expect("tip ok"),
        ChainTip::from_bytes(&copy.to_bytes()).expect("copy ok")
    );
}
