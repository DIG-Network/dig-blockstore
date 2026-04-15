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

/// Default number of blocks per epoch in the stub.
///
/// The real `dig-epoch` crate will derive this from `dig-constants::NetworkConstants`.
/// For now this matches DIG testnet conventions (32 blocks per epoch, analogous to
/// Chia's 32 sub-slots per sub-epoch).
pub const BLOCKS_PER_EPOCH: u64 = 32;

/// Return the inclusive height range `[start, end]` for a given epoch number.
///
/// **Stub implementation:** `start = epoch * BLOCKS_PER_EPOCH`, `end = start + BLOCKS_PER_EPOCH - 1`.
///
/// **Migration:** When the real `dig-epoch` crate ships, this function will be replaced
/// by proper epoch arithmetic that handles variable-size epochs and genesis edge cases.
///
/// **Used by:** [`CAN-006`](../../../docs/requirements/domains/canonical_chain/specs/CAN-006.md)
/// `get_epoch_block_hashes`.
#[must_use]
pub fn epoch_height_range(epoch: u64) -> (u64, u64) {
    let start = epoch.saturating_mul(BLOCKS_PER_EPOCH);
    let end = start.saturating_add(BLOCKS_PER_EPOCH - 1);
    (start, end)
}
