//! Chia **Streamable** wire helpers for gossip / full-node interop ([`SER-003`](../docs/requirements/domains/serialization/specs/SER-003.md)).
//!
//! ## Usage
//!
//! - **Encode:** [`block_to_wire_bytes`] → append to P2P frames or log captures.
//! - **Decode:** [`block_from_wire_bytes`] after stripping transport framing (length prefix, etc.).
//!
//! ## Rationale
//!
//! - **Differs from storage:** [`crate::store::BlockStore`] persists bodies via bincode + zstd ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
//!   [`dig_block::L2Block`] also implements [`serde::Serialize`] for bincode; wire bytes use [`chia_traits::Streamable`]
//!   (length-prefixed lists, big-endian integers) so tooling matches **chia-protocol** / Chia nodes ([`SER-003`] NORMATIVE).
//! - **No compression:** SER-003 §6 — zstd belongs to transport; this layer is pure structure bytes (see tests for no `ZSTD_MAGIC` prefix).
//!
//! ## Upstream types
//!
//! - [`dig_block::L2Block`] implements [`Streamable`] in **dig-block** (manual `impl`, field order matches structs) so
//!   encoding stays on the type-owning crate; this module is a thin [`BlockStoreError`] adapter for dig-blockstore callers.

use chia_traits::Streamable;
use dig_block::L2Block;

use crate::error::BlockStoreError;

/// Serialize a block for Chia-style wire/gossip ([`SER-003`](../docs/requirements/domains/serialization/specs/SER-003.md)).
///
/// **Pipeline:** [`L2Block::stream`] from [`Streamable`] (same as other chia-protocol messages).
///
/// **Errors:** [`BlockStoreError::Serialization`] wraps [`chia_traits::chia_error::Error`] from the encoder.
pub fn block_to_wire_bytes(block: &L2Block) -> Result<Vec<u8>, BlockStoreError> {
    let mut buf = Vec::new();
    block
        .stream(&mut buf)
        .map_err(|e| BlockStoreError::Serialization(format!("wire serialization failed: {e}")))?;
    Ok(buf)
}

/// Deserialize wire bytes from a peer into [`L2Block`] ([`SER-003`](../docs/requirements/domains/serialization/specs/SER-003.md)).
///
/// **Pipeline:** [`L2Block::from_bytes`] — rejects trailing garbage ([`Streamable::from_bytes`] contract).
///
/// **Errors:** Malformed frames map to [`BlockStoreError::Serialization`] (never [`BlockStoreError::Compression`]).
pub fn block_from_wire_bytes(bytes: &[u8]) -> Result<L2Block, BlockStoreError> {
    L2Block::from_bytes(bytes).map_err(|e| {
        BlockStoreError::Serialization(format!("wire deserialization failed: {e}"))
    })
}
