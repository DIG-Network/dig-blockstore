//! [`BlockRecord`] — in-memory block metadata derived from [`dig_block::L2BlockHeader`].
//!
//! **Normative:** [`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md),
//! [`NORMATIVE § TYP-004`](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-004-blockrecord-struct),
//! SPEC §3.2 (`docs/resources/SPEC.md`).
//!
//! ## Persistence
//!
//! **MUST NOT** be written to any RocksDB column family ([`CAC-003`](../../docs/requirements/domains/caching/specs/CAC-003.md)
//! precursor: lives only in the block-record cache, rebuilt from headers on miss). This type intentionally
//! omits [`serde::Serialize`] so accidental “stick it in bincode and put_cf” stays a compile-time smell.
//!
//! ## `in_canonical_chain` vs [`dig_block::BlockStatus`]
//!
//! The historical TYP-004 snippet referenced `BlockStatus::Canonical`, which **does not exist** in current
//! [`dig_block`](https://docs.rs/dig-block) (see ATT-003: `Pending`, `Validated`, `SoftFinalized`,
//! `HardFinalized`, `Orphaned`, `Rejected`). We set
//! **`in_canonical_chain = status.is_canonical()`**, matching dig-block’s predicate: only `Orphaned` and
//! `Rejected` are excluded from canonical-progress views ([`BlockStatus::is_canonical`](dig_block::BlockStatus::is_canonical)).
//!
//! ## `block_size`
//!
//! Headers do not carry the full serialized block byte length needed for storage stats; [`Self::from_header`]
//! leaves [`BlockRecord::block_size`] as **`0`** until a future `with_block_size` / BLK path fills it
//! ([`TYP-004` implementation notes](../../docs/requirements/domains/storage_types/specs/TYP-004.md#implementation-notes)).

use chia_protocol::Bytes32;
use dig_block::{BlockStatus, L2BlockHeader};

/// In-memory summary of block metadata for caches and chain helpers ([`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRecord {
    // --- Identity (NORMATIVE) ---
    /// Header hash ([`L2BlockHeader::hash`], HSH-001 / chia-sha2 in dig-block).
    pub hash: Bytes32,
    pub height: u64,
    pub epoch: u64,
    pub parent_hash: Bytes32,

    // --- Chain position ---
    /// Derived from [`BlockStatus::is_canonical`] — see module docs for mapping rationale.
    pub in_canonical_chain: bool,
    pub status: BlockStatus,

    // --- Statistics ---
    pub timestamp: u64,
    pub proposer_index: u32,
    pub spend_bundle_count: u32,
    pub total_cost: u64,
    pub total_fees: u64,
    pub additions_count: u32,
    pub removals_count: u32,
    /// Serialized block size in bytes when known; [`Self::from_header`] sets **`0`** ([`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md)).
    pub block_size: u64,

    // --- L1 anchor ---
    pub l1_height: u32,
    pub l1_hash: Bytes32,

    // --- State ---
    pub state_root: Bytes32,
}

impl BlockRecord {
    /// Build a record from a header plus lifecycle [`BlockStatus`] ([`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md)).
    ///
    /// **Mapping:** Header fields copy verbatim where names align; [`Self::hash`] uses [`L2BlockHeader::hash`];
    /// [`Self::in_canonical_chain`] uses [`BlockStatus::is_canonical`]; [`Self::block_size`] is **`0`**.
    pub fn from_header(header: &L2BlockHeader, status: BlockStatus) -> Self {
        Self {
            hash: header.hash(),
            height: header.height,
            epoch: header.epoch,
            parent_hash: header.parent_hash,
            in_canonical_chain: status.is_canonical(),
            status,
            timestamp: header.timestamp,
            proposer_index: header.proposer_index,
            spend_bundle_count: header.spend_bundle_count,
            total_cost: header.total_cost,
            total_fees: header.total_fees,
            additions_count: header.additions_count,
            removals_count: header.removals_count,
            block_size: 0,
            l1_height: header.l1_height,
            l1_hash: header.l1_hash,
            state_root: header.state_root,
        }
    }
}
