//! Canonical chain index logic (dual layer: mmap hot + `CF_CANONICAL` cold).
//!
//! **Spec:** [`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md), [`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md).

/// Owns in-memory / on-disk views of the height→hash map (`docs/resources/SPEC.md` — dense height index).
#[derive(Debug, Default)]
pub struct CanonicalIndex {}
