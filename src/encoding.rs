//! Key encoding helpers for RocksDB keys (hash, height, epoch, metadata).
//!
//! **Normative**
//! - [`KEY-001`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md) — hash keys
//! - [`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md) — height keys
//! - [`KEY-003`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md) — epoch keys
//! - [`KEY-004`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md) — metadata UTF-8 keys
//! - Crate-root re-exports: [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md)
//!
//! **Rationale:** Big-endian `u64` keys ([`height_key`], [`epoch_key`]) preserve lexicographic order
//! equal to numeric order, enabling efficient range scans. Hash keys are raw [`Bytes32`] with no prefix
//! ([`chia_protocol::Bytes32`]).

use chia_protocol::Bytes32;

/// Raw 32-byte RocksDB key for `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED` ([`KEY-001`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md)).
///
/// **Contract:** The returned array is **exactly** the [`Bytes32`] octets — no length prefix,
/// type tag, or endian reinterpretation. This matches NORMATIVE `key = block_hash.as_ref() → [u8; 32]`.
///
/// **Usage:** Pass `hash_key(h).as_slice()` to RocksDB APIs expecting `&[u8]` ([`crate::store::BlockStore`]).
///
/// **Zero-copy:** The slice borrows the same 32 bytes held inside `Bytes32` (fixed-size wire type from Chia / DIG stack).
#[must_use]
pub fn hash_key(hash: &Bytes32) -> &[u8; 32] {
    hash.as_ref()
        .try_into()
        .expect("Bytes32 must be exactly 32 bytes (KEY-001)")
}

/// Encode a chain height as an 8-byte **big-endian** key for [`crate::constants::CF_CANONICAL`] ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)).
///
/// **Sort invariant:** For `a < b`, `height_key(a).as_slice() < height_key(b).as_slice()` in bytewise order, so
/// RocksDB’s default comparator iterates heights in ascending numeric order (required for range scans and reorg walks).
///
/// **Decode:** Use [`decode_height_key`] after reads (symmetric to [`decode_epoch_key`] for checkpoints).
///
/// **Fixed width:** Always exactly 8 bytes — no VLQ or length prefix.
#[must_use]
pub fn height_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}

/// Decode a height key produced by [`height_key`] ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)).
#[must_use]
pub fn decode_height_key(key: &[u8; 8]) -> u64 {
    u64::from_be_bytes(*key)
}

/// Encode an epoch number as an 8-byte **big-endian** key for [`crate::constants::CF_CHECKPOINTS`]
/// ([`KEY-003`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md),
/// [`NORMATIVE` §KEY-003](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-003-epoch-keys-8-bytes-big-endian)).
///
/// **Wire shape:** Identical octets to [`height_key`] for the same `u64` ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md));
/// the separate name documents call-site intent (checkpoint epochs vs canonical heights) and leaves room for future newtypes.
///
/// **Sort invariant:** For `a < b`, `epoch_key(a).as_slice() < epoch_key(b).as_slice()` in bytewise order, matching RocksDB’s
/// default comparator — required for epoch-range scans (e.g. future [`CKP-004`](../docs/requirements/domains/checkpoint_storage/specs/CKP-004_get_checkpoints_in_range.md)).
///
/// **Decode:** Use [`decode_epoch_key`] after reads (symmetric to [`decode_height_key`] for canonical heights).
///
/// **Fixed width:** Always exactly 8 bytes — no VLQ or length prefix.
#[must_use]
pub fn epoch_key(epoch: u64) -> [u8; 8] {
    epoch.to_be_bytes()
}

/// Decode an epoch key produced by [`epoch_key`] ([`KEY-003`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md)).
///
/// **Contract:** Input MUST be exactly the 8 bytes returned by [`epoch_key`] for some `u64`; this is the inverse of
/// `u64::to_be_bytes` / `u64::from_be_bytes` and matches [`decode_height_key`]’s numeric interpretation.
#[must_use]
pub fn decode_epoch_key(key: &[u8; 8]) -> u64 {
    u64::from_be_bytes(*key)
}

/// UTF-8 bytes for a metadata key name in [`crate::constants::CF_METADATA`]
/// ([`KEY-004`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md),
/// [`NORMATIVE` §KEY-004](../docs/requirements/domains/key_encoding/NORMATIVE.md#key-004-metadata-keys-variable-utf-8)).
///
/// **Contract:** Returns `name.as_bytes()` — the exact UTF-8 encoding of `name`. No length prefix, no type tag,
/// no NUL terminator. Key length equals the UTF-8 byte length (variable; unlike fixed-width hash/height/epoch keys).
///
/// **Well-known keys:** Prefer [`crate::constants::META_TIP`], [`crate::constants::META_GENESIS_HASH`],
/// [`crate::constants::META_MIN_HEIGHT`], [`crate::constants::META_SCHEMA_VERSION`],
/// [`crate::constants::META_ZSTD_DICT`] at call sites so metadata names stay centralized ([`TYP-002`](../docs/requirements/domains/storage_types/specs/TYP-002.md)).
///
/// **Usage:** Pass `metadata_key(name)` (or `META_*.as_bytes()`) to RocksDB `get_cf` / `put_cf` for `CF_METADATA`
/// ([`crate::store::BlockStore`]). Human-readable ASCII names aid `ldb` inspection per KEY-004 implementation notes.
///
/// **Sort order:** Unlike canonical height keys, metadata rows are looked up by **exact key**; lexicographic order is
/// not part of the storage contract for this family.
#[must_use]
pub fn metadata_key(name: &str) -> &[u8] {
    name.as_bytes()
}
