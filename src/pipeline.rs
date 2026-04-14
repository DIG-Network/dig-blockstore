//! Async batched write pipeline ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md), `docs/resources/SPEC.md` write pipeline section).
//!
//! **Runtime:** [`tokio`](https://docs.rs/tokio) channel + `rocksdb::WriteBatch`.

/// Bounded async ingress queue draining into RocksDB batches (shell type for STR-002).
#[derive(Debug, Default)]
pub struct BlockWritePipeline {}
