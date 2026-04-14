//! # Interim `dig-epoch` stub (bootstrap only)
//!
//! The normative architecture (`docs/resources/SPEC.md` §1.2, `start.md` “Hard
//! Requirements”) states that **epoch arithmetic lives in `dig-epoch`**. That
//! crate is not yet available on crates.io and the standalone `dig-epoch`
//! documentation repository does not yet ship a Rust library target.
//!
//! ## Why this stub exists
//!
//! [`STR-001`](../../../docs/requirements/domains/crate_structure/specs/STR-001.md)
//! requires `Cargo.toml` to declare `dig-epoch = "0.1"` **and** for `cargo check`
//! to succeed without missing-crate errors. A tiny, in-tree library with the
//! correct **package name** satisfies resolution today without pretending to
//! implement epoch logic.
//!
//! ## Migration path
//!
//! When the real `dig-epoch` crate is published (or vendored), delete this stub
//! and switch the parent manifest to a registry/path dependency pointing at the
//! real implementation. Downstream code in `dig-blockstore` must then call the
//! real APIs (`epoch_for_block_height`, `first_height_in_epoch`, etc.).
#![forbid(unsafe_code)]

/// Marker type reserved for future wiring tests. Not used by production code yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DigEpochStub;
