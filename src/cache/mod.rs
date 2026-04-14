//! In-memory caching layer (sharded LRU + startup warming).
//!
//! **Normative:** [`STR-002`](../../docs/requirements/domains/crate_structure/specs/STR-002.md).
//! **Behavior:** [`CAC-001`…`CAC-006`](../../docs/requirements/domains/caching/NORMATIVE.md).

pub mod sharded;
pub mod warming;
