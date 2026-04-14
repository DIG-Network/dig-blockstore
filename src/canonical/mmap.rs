//! Memory-mapped `canonical.bin` dense hash array ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md)).
//!
//! Uses [`memmap2`](https://docs.rs/memmap2) for O(1) lookups; see [`crate::canonical::index`] for coordination.

/// Handle to the mmap’d canonical height file (placeholder).
#[derive(Debug, Default)]
pub struct CanonicalMmap {}
