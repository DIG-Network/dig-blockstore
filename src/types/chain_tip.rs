//! [`ChainTip`] — fixed **40-byte** encoding of canonical tip **hash + height**.
//!
//! ## Requirements trace
//!
//! - **Spec / acceptance:** [`TYP-006`](../../docs/requirements/domains/storage_types/specs/TYP-006.md)
//! - **NORMATIVE:** [`storage_types/NORMATIVE.md`](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-006-chaintip-struct)
//! - **Wire key:** value stored under [`crate::constants::META_TIP`] in `CF_METADATA` ([`TYP-002`](../../docs/requirements/domains/storage_types/specs/TYP-002.md)).
//! - **Broader context:** [`SPEC.md`](../../docs/resources/SPEC.md) Section 3.4 (chain tip record).
//!
//! ## Design rationale
//!
//! - **No length prefix:** Tip payloads are always 40 bytes, so `from_bytes` is O(1) and cannot be confused
//!   with truncated reads if the DB value size is wrong (caller should still validate `len == 40` at the
//!   storage layer; see `load_tip` in [`crate::store`]).
//! - **Little-endian height:** Matches native `u64` byte order on all tier-1 DIG targets and matches the
//!   TYP-006 / SPEC layout table (`bytes[32..40]`).
//! - **`Copy`:** The struct is 40 bytes on the stack; copying is cheaper than `Arc` or heap indirection for
//!   hot-path `tip()` accessors.
//!
//! ## Errors
//!
//! Wrong-length slices are reported as [`crate::error::BlockStoreError::Serialization`] (see
//! [`ERR-001`](../../docs/requirements/domains/error_types/specs/ERR-001_blockstore_error_enum.md)): the
//! error enum does not carry a dedicated “invalid tip bytes” variant, and fixed-width parse failures are
//! treated like other malformed serialized blobs.

use chia_protocol::Bytes32;

use crate::error::BlockStoreError;

/// Canonical chain tip: L2 header identity hash at the current peak plus its height.
///
/// **Construction:** callers typically obtain this from [`crate::store::BlockStore::tip`] after
/// [`crate::store::BlockStore::init_genesis`] or future tip-update APIs ([`CAN-007`](../../docs/requirements/domains/canonical_chain/specs/CAN-007.md) — not implemented here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainTip {
    /// Hash of the tip block (header identity hash).
    pub hash: Bytes32,
    /// Height of the tip block.
    pub height: u64,
}

impl ChainTip {
    /// Encode to 40 bytes: `hash` raw (`Bytes32` as `[u8; 32]`) || `height` (`u64`, little-endian).
    ///
    /// **Usage:** persist as the value for [`crate::constants::META_TIP`]; see [`Self::from_bytes`] for decode.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 40] {
        let mut buf = [0u8; 40];
        buf[0..32].copy_from_slice(self.hash.as_ref());
        buf[32..40].copy_from_slice(&self.height.to_le_bytes());
        buf
    }

    /// Decode from exactly 40 bytes ([`TYP-006`](../../docs/requirements/domains/storage_types/specs/TYP-006.md)).
    ///
    /// **Returns:** [`Err`] with [`BlockStoreError::Serialization`] if `bytes.len() != 40`.
    /// **Invariant:** On `Ok`, `from_bytes(&tip.to_bytes()) == Ok(tip)`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlockStoreError> {
        if bytes.len() != 40 {
            return Err(BlockStoreError::Serialization(format!(
                "ChainTip requires 40 bytes, got {}",
                bytes.len()
            )));
        }
        let mut hb = [0u8; 32];
        hb.copy_from_slice(&bytes[0..32]);
        let hash = Bytes32::new(hb);
        let mut hb8 = [0u8; 8];
        hb8.copy_from_slice(&bytes[32..40]);
        let height = u64::from_le_bytes(hb8);
        Ok(Self { hash, height })
    }
}
