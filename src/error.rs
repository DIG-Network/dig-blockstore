//! `BlockStore` error surface (`BlockStoreError`).
//!
//! **Spec / requirements**
//! - Placeholder variants satisfy [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md)
//!   (“`error.rs` exists with `BlockStoreError`”).
//! - Full variant set and `Display` quality are [`ERR-001`…`ERR-003`](../docs/requirements/domains/error_types/NORMATIVE.md).
//!
//! **Design decision:** Start with a minimal enum so the module tree compiles; expand
//! variants only when behavior requirements land (avoid speculative error taxonomy).

use thiserror::Error;

/// Top-level error type for persistent block storage operations.
#[derive(Debug, Error)]
pub enum BlockStoreError {
    /// Reserved until store methods return real failures ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md) onward).
    #[error("block store operation not implemented yet")]
    NotImplemented,
}
