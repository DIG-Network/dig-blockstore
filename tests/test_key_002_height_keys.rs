//! # KEY-002 — `CF_CANONICAL` height keys: 8-byte **big-endian** `u64`
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec: [`KEY-002_height_keys.md`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)
//! - NORMATIVE: [`NORMATIVE.md` (KEY-002)](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-002-height-keys-8-bytes-big-endian)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/key_encoding/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Encoding:** [`dig_blockstore::height_key`] is the crate API (re-exported per [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md));
//!   KEY-002 prose uses `encode_height_key` — same semantics ([`KEY-002` implementation notes](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)).
//! - **Order:** Bytewise sort of keys must match numeric height order so RocksDB iteration over [`CF_CANONICAL`](dig_blockstore::CF_CANONICAL)
//!   walks the chain forward without a custom comparator ([`store.rs`](../src/store.rs) uses `height_key` in writes).
//! - **Round-trip:** [`dig_blockstore::decode_height_key`] inverts [`height_key`], matching the spec’s decode helper and mirroring [`decode_epoch_key`](dig_blockstore::decode_epoch_key).

#![forbid(unsafe_code)]

use dig_blockstore::{decode_height_key, height_key};

#[test]
fn test_zero_height_eight_zero_bytes() {
    // **Proves:** KEY-002 test plan §1 — height 0 → `00`×8 (table row “0”).
    assert_eq!(height_key(0), [0u8; 8]);
}

#[test]
fn test_height_one_least_significant_byte() {
    // **Proves:** KEY-002 acceptance §2 + test plan §2 — big-endian: only LSB set.
    assert_eq!(height_key(1), [0, 0, 0, 0, 0, 0, 0, 1]);
}

#[test]
fn test_height_255_and_256_big_endian() {
    // **Proves:** KEY-002 sort table rows for255 / 256 — high byte advances only when lower bytes wrap.
    assert_eq!(height_key(255), [0, 0, 0, 0, 0, 0, 0, 0xFF]);
    assert_eq!(height_key(256), [0, 0, 0, 0, 0, 0, 1, 0]);
}

#[test]
fn test_height_1000_hex_table() {
    // **Proves:** KEY-002 table row `1000` → `… 03 E8`.
    assert_eq!(height_key(1000), [0, 0, 0, 0, 0, 0, 0x03, 0xE8]);
}

#[test]
fn test_u64_max_all_ff() {
    // **Proves:** KEY-002 test plan §3 — `u64::MAX` fills the key.
    assert_eq!(height_key(u64::MAX), [0xFF; 8]);
}

#[test]
fn test_sort_order_matches_numeric_order() {
    // **Proves:** KEY-002 acceptance §3 + test plan §4 — lex sort on key bytes == numeric sort on heights.
    let heights = [0u64, 1, 2, 255, 256, 1000, u64::MAX - 1, u64::MAX];
    let mut keys: Vec<[u8; 8]> = heights.iter().copied().map(height_key).collect();
    keys.sort();
    let decoded: Vec<u64> = keys.iter().map(decode_height_key).collect();
    let mut sorted_heights = heights.to_vec();
    sorted_heights.sort();
    assert_eq!(decoded, sorted_heights);
}

#[test]
fn test_round_trip_sample_heights() {
    // **Proves:** KEY-002 acceptance §4 + test plan §5.
    for h in [0u64, 1, 42, 1000, 1 << 48, u64::MAX] {
        assert_eq!(decode_height_key(&height_key(h)), h);
    }
}

#[test]
fn test_round_trip_exhaustive_u16() {
    // **Proves:** Dense check for a large numeric range without full u64 enumeration (fast CI).
    for h in 0u64..=u16::MAX as u64 {
        assert_eq!(decode_height_key(&height_key(h)), h);
    }
}

#[test]
fn test_length_always_eight() {
    // **Proves:** KEY-002 acceptance §1 — fixed-width key for index / prefix bloom assumptions.
    assert_eq!(height_key(0).len(), 8);
    assert_eq!(height_key(u64::MAX).len(), 8);
}

#[test]
fn test_little_endian_would_violate_height_order() {
    // **Proves:** KEY-002 test plan §6 — LE keys do not sort like integers (motivation for big-endian).
    let a = 1u64;
    let b = 256u64;
    assert!(
        height_key(a).as_slice() < height_key(b).as_slice(),
        "BE must order 1 before 256 lexicographically"
    );
    let le_a = a.to_le_bytes();
    let le_b = b.to_le_bytes();
    assert!(
        le_b.as_slice() < le_a.as_slice(),
        "with LE, 256’s key sorts before 1’s key — breaking canonical iteration order"
    );
}
