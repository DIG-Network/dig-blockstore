//! Async batched write pipeline ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)).
//!
//! Pipeline logic lives in [`crate::store::BlockStore::put_pipelined`] and the
//! `run_write_pipeline` background task spawned on first call.
