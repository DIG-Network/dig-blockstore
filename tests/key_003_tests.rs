//! # KEY-003 — `CF_CHECKPOINTS` epoch keys: 8-byte **big-endian** `u64`
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec: [`KEY-003_epoch_keys.md`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md)
//! - NORMATIVE: [`NORMATIVE.md` (KEY-003)](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-003-epoch-keys-8-bytes-big-endian)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/key_encoding/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Encoding:** [`dig_blockstore::epoch_key`] / [`dig_blockstore::decode_epoch_key`] are the public API ([`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md)).
//!   KEY-003 prose uses `encode_epoch_key` — identical semantics ([`KEY-003`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md)).
//! - **Column family:** Keys index rows in [`CF_CHECKPOINTS`](dig_blockstore::CF_CHECKPOINTS) ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md));
//!   [`typ_005_tests.rs`](typ_005_tests.rs) and production `store` code use the same shape for checkpoint puts/gets.
//! - **Sort order:** Bytewise order must track numeric epoch order for range scans (e.g. future [`CKP-004`](../docs/requirements/domains/checkpoint_storage/specs/CKP-004_get_checkpoints_in_range.md)).
//! - **Consistency with heights:** Wire format matches [`height_key`](dig_blockstore::height_key) ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)); separate functions preserve call-site intent per KEY-003 implementation notes.

#![forbid(unsafe_code)]

use dig_blockstore::{decode_epoch_key, epoch_key, height_key};

#[test]
fn test_zero_epoch_eight_zero_bytes() {
    // **Proves:** KEY-003 test plan §1.
    assert_eq!(epoch_key(0), [0u8; 8]);
}

#[test]
fn test_epoch_one_least_significant_byte() {
    // **Proves:** KEY-003 acceptance §2 + test plan §2 — same BE pattern as height1.
    assert_eq!(epoch_key(1), [0, 0, 0, 0, 0, 0, 0, 1]);
}

#[test]
fn test_u64_max_all_ff() {
    // **Proves:** KEY-003 test plan §3.
    assert_eq!(epoch_key(u64::MAX), [0xFF; 8]);
}

#[test]
fn test_sort_order_matches_numeric_order() {
    // **Proves:** KEY-003 acceptance §3 + test plan §4 — explicit epoch list from spec.
    let epochs = [0u64, 1, 10, 100, 1000, u64::MAX];
    let mut keys: Vec<[u8; 8]> = epochs.iter().copied().map(epoch_key).collect();
    keys.sort();
    let decoded: Vec<u64> = keys.iter().map(decode_epoch_key).collect();
    let mut sorted = epochs.to_vec();
    sorted.sort();
    assert_eq!(decoded, sorted);
}

#[test]
fn test_round_trip_sample_epochs() {
    // **Proves:** KEY-003 acceptance §4 + test plan §5.
    for e in [0u64, 1, 42, 1000, 1 << 40, u64::MAX] {
        assert_eq!(decode_epoch_key(&epoch_key(e)), e);
    }
}

#[test]
fn test_round_trip_exhaustive_u16() {
    // **Proves:** Dense coverage (same strategy as KEY-002 tests).
    for e in 0u64..=u16::MAX as u64 {
        assert_eq!(decode_epoch_key(&epoch_key(e)), e);
    }
}

#[test]
fn test_length_always_eight() {
    // **Proves:** KEY-003 acceptance §1 — fixed-width 8-byte keys.
    assert_eq!(epoch_key(0).len(), 8);
    assert_eq!(epoch_key(u64::MAX).len(), 8);
}

#[test]
fn test_epoch_key_matches_height_key_for_same_u64() {
    // **Proves:** KEY-003 test plan §6 — identical octets, different named entry points ([`KEY-003` implementation notes](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md)).
    for v in [0u64, 1, 255, 256, 10_000, u64::MAX / 3] {
        assert_eq!(epoch_key(v), height_key(v));
    }
}
