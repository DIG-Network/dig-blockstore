//! [`ChainTip`] — 40-byte encoding of peak hash + height ([`TYP-006`](../../docs/requirements/domains/storage_types/specs/TYP-006.md)).
//!
//! Persisted under [`crate::constants::META_TIP`].

use chia_protocol::Bytes32;

use crate::error::BlockStoreError;

/// Canonical chain tip (hash + height) — see `docs/resources/SPEC.md` / TYP-006 layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainTip {
    /// Hash of the tip block (header identity hash).
    pub hash: Bytes32,
    /// Height of the tip block.
    pub height: u64,
}

impl ChainTip {
    /// Encode to 40 bytes: `hash (32)` || `height` (u64 little-endian).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 40] {
        let mut buf = [0u8; 40];
        buf[0..32].copy_from_slice(self.hash.as_ref());
        buf[32..40].copy_from_slice(&self.height.to_le_bytes());
        buf
    }

    /// Decode from exactly 40 bytes ([`TYP-006`](../../docs/requirements/domains/storage_types/specs/TYP-006.md) acceptance).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlockStoreError> {
        if bytes.len() != 40 {
            return Err(BlockStoreError::InvalidData(format!(
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
