//! Lightweight storage-facing types (records, tip, stats).
//!
//! **Normative:** [`STR-002`](../../docs/requirements/domains/crate_structure/specs/STR-002.md) module layout.
//! **Block types** (`L2Block`, …) remain in [`dig_block`](https://docs.rs/dig-block) per project hard requirements.

pub mod block_record;
pub mod chain_tip;
pub mod storage_stats;
pub mod stored_checkpoint;

pub use block_record::BlockRecord;
pub use chain_tip::ChainTip;
pub use storage_stats::StorageStats;
pub use stored_checkpoint::StoredCheckpoint;
