//! # KEY-004 — `CF_METADATA` keys: **variable-length UTF-8** (`&str` → `&[u8]`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec: [`KEY-004_metadata_keys.md`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md)
//! - NORMATIVE: [`NORMATIVE.md` (KEY-004)](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-004-metadata-keys-variable-utf-8)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/key_encoding/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Encoding:** [`dig_blockstore::metadata_key`] is the crate-root API ([`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md)).
//!   The KEY-004 spec’s `encode_metadata_key` is the same operation ([`KEY-004`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md)).
//! - **Column family:** Keys are used with [`CF_METADATA`](dig_blockstore::CF_METADATA) ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md));
//!   well-known names live as `META_*` constants ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
//!   Production code (e.g. [`BlockStore`](dig_blockstore::BlockStore) metadata writes) passes the same bytes via
//!   `META_*.as_bytes()` or [`metadata_key`]; this file locks the **normative byte identity** of those keys.
//! - **UTF-8:** Rust `str` is always valid UTF-8; [`str::as_bytes`] is the exact on-disk encoding — no NUL terminator,
//!   no length prefix, no endianness reinterpretation ([`NORMATIVE`](../docs/requirements/domains/key_encoding/NORMATIVE.md)).

#![forbid(unsafe_code)]

use dig_blockstore::{
    metadata_key, CF_METADATA, META_GENESIS_HASH, META_MIN_HEIGHT, META_SCHEMA_VERSION, META_TIP,
    META_ZSTD_DICT,
};

/// Expected UTF-8 bytes for each well-known metadata key (KEY-004 test plan §1).
///
/// **Rationale:** Explicit byte tables catch accidental renames in `META_*` constants or drift from the
/// human-readable names tools like `ldb` display.
const EXPECTED_TIP: &[u8] = &[0x74, 0x69, 0x70];
const EXPECTED_GENESIS_HASH: &[u8] = b"genesis_hash";
const EXPECTED_MIN_HEIGHT: &[u8] = b"min_height";
const EXPECTED_SCHEMA_VERSION: &[u8] = b"schema_version";
const EXPECTED_ZSTD_DICT: &[u8] = b"zstd_dict";

#[test]
fn test_well_known_keys_match_explicit_utf8_tables() {
    // **Proves:** KEY-004 test plan §1 + acceptance §1 — exact encodings for all five names.
    assert_eq!(metadata_key(META_TIP), EXPECTED_TIP);
    assert_eq!(metadata_key(META_GENESIS_HASH), EXPECTED_GENESIS_HASH);
    assert_eq!(metadata_key(META_MIN_HEIGHT), EXPECTED_MIN_HEIGHT);
    assert_eq!(metadata_key(META_SCHEMA_VERSION), EXPECTED_SCHEMA_VERSION);
    assert_eq!(metadata_key(META_ZSTD_DICT), EXPECTED_ZSTD_DICT);
}

#[test]
fn test_variable_length_examples_from_spec() {
    // **Proves:** KEY-004 test plan §2 + acceptance §3 — different logical names → different byte lengths.
    assert_eq!(metadata_key(META_TIP).len(), 3);
    assert_eq!(metadata_key(META_SCHEMA_VERSION).len(), 14);
    assert!(
        metadata_key(META_GENESIS_HASH).len() != metadata_key(META_TIP).len(),
        "genesis_hash and tip must not collide in length (KEY-004 variable-length contract)"
    );
}

#[test]
fn test_no_trailing_nul_on_well_known_keys() {
    // **Proves:** KEY-004 test plan §3 + acceptance §4 — encoder does not append `0x00`.
    //
    // **Note:** We assert the *last* byte is non-NUL; well-known keys are ASCII words with no interior NULs either.
    for (label, bytes) in [
        ("META_TIP", metadata_key(META_TIP)),
        ("META_GENESIS_HASH", metadata_key(META_GENESIS_HASH)),
        ("META_MIN_HEIGHT", metadata_key(META_MIN_HEIGHT)),
        ("META_SCHEMA_VERSION", metadata_key(META_SCHEMA_VERSION)),
        ("META_ZSTD_DICT", metadata_key(META_ZSTD_DICT)),
    ] {
        assert!(
            bytes.last().is_some_and(|b| *b != 0),
            "{label}: encoded key must not end with NUL (KEY-004)"
        );
    }
}

#[test]
fn test_well_known_encodings_are_pairwise_distinct() {
    // **Proves:** KEY-004 test plan §4 — no two well-known keys map to the same byte sequence.
    let keys = [
        metadata_key(META_TIP),
        metadata_key(META_GENESIS_HASH),
        metadata_key(META_MIN_HEIGHT),
        metadata_key(META_SCHEMA_VERSION),
        metadata_key(META_ZSTD_DICT),
    ];
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            assert_ne!(
                keys[i], keys[j],
                "metadata keys {} and {} must differ on the wire",
                i, j
            );
        }
    }
}

#[test]
fn test_meta_constants_string_values_and_utf8_validity() {
    // **Proves:** KEY-004 test plan §5 + acceptance §2/§5 — constants exist at crate root with expected literals.
    assert_eq!(META_TIP, "tip");
    assert_eq!(META_GENESIS_HASH, "genesis_hash");
    assert_eq!(META_MIN_HEIGHT, "min_height");
    assert_eq!(META_SCHEMA_VERSION, "schema_version");
    assert_eq!(META_ZSTD_DICT, "zstd_dict");

    for (name, s) in [
        ("META_TIP", META_TIP),
        ("META_GENESIS_HASH", META_GENESIS_HASH),
        ("META_MIN_HEIGHT", META_MIN_HEIGHT),
        ("META_SCHEMA_VERSION", META_SCHEMA_VERSION),
        ("META_ZSTD_DICT", META_ZSTD_DICT),
    ] {
        assert!(
            core::str::from_utf8(s.as_bytes()).is_ok(),
            "{name} must be valid UTF-8 (KEY-004 / Rust str invariant)"
        );
    }
}

#[test]
fn test_metadata_key_matches_str_as_bytes_contract() {
    // **Proves:** NORMATIVE `key = name.as_bytes()` — [`metadata_key`] is a thin, deterministic view of the same bytes.
    let name = "custom_debug_key";
    assert_eq!(metadata_key(name), name.as_bytes());
    assert_eq!(metadata_key(META_TIP), META_TIP.as_bytes());
}

#[test]
fn test_non_ascii_utf8_round_trips_through_utf8_decode() {
    // **Proves:** KEY-004 spec — ASCII-only well-known keys, but **full UTF-8** is allowed for arbitrary metadata names.
    //
    // **How:** Multi-byte scalar `λ` (U+03BB) has a stable UTF-8 encoding; RocksDB stores opaque bytes — callers must
    // agree on UTF-8 for interoperability with `str`-based APIs.
    let name = "λ";
    let key = metadata_key(name);
    assert_eq!(name.len(), 2, "U+03BB is two UTF-8 code units");
    assert_eq!(
        name.chars().count(),
        1,
        "one Unicode scalar, multiple UTF-8 bytes"
    );
    assert_eq!(
        key.len(),
        name.len(),
        "metadata_key must expose the same bytes as str::as_bytes"
    );
    assert_eq!(core::str::from_utf8(key).expect("valid UTF-8"), name);
}

#[test]
fn test_empty_name_is_zero_length_key() {
    // **Proves:** Variable-length contract edge — empty string → empty key slice (no framing bytes added).
    assert!(metadata_key("").is_empty());
}

#[test]
fn test_cf_metadata_constant_documents_intended_family() {
    // **Proves:** Semantic link — these keys are defined for use in `CF_METADATA`, not hash/height families.
    assert_eq!(CF_METADATA, "metadata");
}
