//! [`ReorgResult`] — outcome of a chain reorganization ([`ROR-003`](../../docs/requirements/domains/rollback_reorg/specs/ROR-003.md)).

use chia_protocol::Bytes32;

use crate::types::ChainTip;

/// Result of an [`apply_reorg`](crate::store::BlockStore::apply_reorg) operation.
///
/// Contains the hashes of reverted blocks (old canonical chain above the ancestor),
/// the hashes of applied blocks (new canonical chain), and the new chain tip.
///
/// # Chia analogy
///
/// Chia's `ReceiveBlockResult` carries similar fork-switch metadata in the
/// `fork_height` and `rolled_back_records` fields. DIG's `ReorgResult` is more
/// explicit, listing every reverted and applied hash for the caller to process
/// (e.g., re-evaluating mempool transactions, notifying subscribers).
#[derive(Debug, Clone)]
pub struct ReorgResult {
    /// Block hashes removed from the canonical chain, in **descending** height order (tip first).
    pub reverted: Vec<Bytes32>,
    /// Block hashes added to the canonical chain, in **ascending** height order (oldest first).
    pub applied: Vec<Bytes32>,
    /// The new chain tip after the reorg (last block in `applied`).
    pub new_tip: ChainTip,
}
