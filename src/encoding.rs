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

/// Encode a chain height as an 8-byte **big-endian** key ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)).
#[must_use]
pub fn height_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}

/// Encode an epoch number as an 8-byte **big-endian** key for `CF_CHECKPOINTS` ([`KEY-003`](../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md)).
#[must_use]
pub fn epoch_key(epoch: u64) -> [u8; 8] {
    epoch.to_be_bytes()
}

/// Decode an epoch key produced by [`epoch_key`].
#[must_use]
pub fn decode_epoch_key(key: &[u8; 8]) -> u64 {
    u64::from_be_bytes(*key)
}

/// UTF-8 bytes for a metadata key name in `CF_METADATA` ([`KEY-004`](../docs/requirements/domains/key_encoding/specs/KEY-004_metadata_keys.md)).
#[must_use]
pub fn metadata_key(name: &str) -> &[u8] {
    name.as_bytes()
}
