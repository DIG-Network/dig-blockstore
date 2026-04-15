//! Canonical height → hash index (RocksDB + mmap fast path).
//!
//! **Normative:** [`STR-002`](../../docs/requirements/domains/crate_structure/specs/STR-002.md), [`CAN-*`](../../docs/requirements/domains/canonical_chain/NORMATIVE.md).

pub mod index;
pub mod mmap;

pub use mmap::{CanonicalDenseFile, CANONICAL_BIN_FILE};
