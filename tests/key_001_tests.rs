//! # KEY-001 — Raw 32-byte hash keys for `CF_BLOCKS` / `CF_HEADERS` / `CF_ATTESTED`
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec: [`KEY-001_hash_keys.md`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md)
//! - NORMATIVE: [`NORMATIVE.md` (KEY-001)](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-001-hash-keys-32-bytes)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/key_encoding/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Identity encoding:** [`dig_blockstore::hash_key`] (crate-root re-export per [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md))
//!   must expose the [`chia_protocol::Bytes32`] payload as exactly **32 bytes** with **no prefix, length field, or
//!   suffix** ([`KEY-001`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md) summary).
//! - **Column families:** The same function is used for all hash-keyed families ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md));
//!   this file proves the **encoding contract**; `store.rs` proves wiring.
//! - **Round-trip:** Rebuilding [`Bytes32`] from the key octets recovers the original value, so RocksDB round-trips
//!   hashes without a secondary mapping.
//! - **Spec naming:** KEY-001 prose uses `encode_hash_key`; the shipped API is [`hash_key`](dig_blockstore::hash_key)
//!   (see spec implementation note below).

#![forbid(unsafe_code)]

use chia_protocol::Bytes32;
use dig_blockstore::hash_key;

#[test]
fn test_zero_hash_all_zero_bytes() {
    // **Proves:** KEY-001 test plan §1 — `Bytes32::default()` (all zeros) yields a 32-byte zero key.
    let h = Bytes32::default();
    let k = hash_key(&h);
    assert_eq!(k.len(), 32);
    assert!(k.iter().all(|&b| b == 0));
}

#[test]
fn test_known_hash_exact_bytes() {
    // **Proves:** KEY-001 test plan §2 — deterministic pattern matches byte-for-byte (no endian swap, no tag).
    let mut arr = [0u8; 32];
    for (i, b) in arr.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    let h = Bytes32::new(arr);
    assert_eq!(hash_key(&h).as_slice(), arr.as_slice());
}

#[test]
fn test_length_always_32() {
    // **Proves:** KEY-001 test plan §3 + acceptance §1 — fixed-width keys for bloom / index predictability.
    for fill in [0x00u8, 0xFF, 0x5A] {
        let h = Bytes32::new([fill; 32]);
        assert_eq!(hash_key(&h).len(), 32);
    }
}

#[test]
fn test_round_trip_bytes32() {
    // **Proves:** KEY-001 test plan §4 + acceptance §4 — decode path used after `get` is identity.
    let h = Bytes32::new([0xAB; 32]);
    let k = hash_key(&h);
    let back = Bytes32::new(*k);
    assert_eq!(back, h);
}

#[test]
fn test_distinct_hashes_distinct_keys() {
    // **Proves:** KEY-001 test plan §5 — injective on the hash domain (collisions only if `Bytes32` collides).
    let a = Bytes32::new([1u8; 32]);
    let b = Bytes32::new([2u8; 32]);
    assert_ne!(hash_key(&a), hash_key(&b));
}

#[test]
fn test_key_bytes_match_as_ref() {
    // **Proves:** KEY-001 acceptance §2 — returned bytes are identical to the raw hash octets (`AsRef` view).
    let h = Bytes32::new([0xCD; 32]);
    assert_eq!(hash_key(&h).as_slice(), h.as_ref());
}

#[test]
fn test_no_framing_implied_by_length_only() {
    // **Proves:** KEY-001 acceptance §3 — key material length is exactly the hash width (32), not 32+N.
    // If a future change added a prefix, this would need updating and would fail `round_trip` / equality tests.
    let h = Bytes32::new([0x77; 32]);
    let k = hash_key(&h);
    assert_eq!(core::mem::size_of_val(k), 32);
    assert_eq!(k.len(), h.as_ref().len());
}
