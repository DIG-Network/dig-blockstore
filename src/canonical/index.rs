//! Canonical chain index coordination ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md), [`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md)).
//!
//! **Current state:** Height→hash dual storage (`canonical.bin` via [`crate::canonical::mmap::CanonicalBin`],
//! [`crate::constants::CF_CANONICAL`]) is owned by [`crate::store::BlockStoreInner`]. This module keeps the
//! historical `CanonicalIndex` placeholder for STR-002 layout until higher-level chain APIs consolidate here.

/// Placeholder for future orchestration types ([`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md) `set_canonical`).
#[derive(Debug, Default)]
pub struct CanonicalIndex {}
