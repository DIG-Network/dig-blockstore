//! [`ChainTip`] — 40-byte encoding of peak hash + height ([`TYP-006`](../../docs/requirements/domains/storage_types/specs/TYP-006.md)).
//!
//! Persisted under [`crate::constants::META_TIP`].

/// Canonical chain tip (hash + height).
#[derive(Debug, Clone, Default)]
pub struct ChainTip {}
