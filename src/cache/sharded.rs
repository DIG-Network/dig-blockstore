//! Sharded LRU cache for hot blocks / headers ([`CAC-001`](../../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
//!
//! **STR-002:** Provide the type shell; sharding policy arrives with implementation.

/// Placeholder for the production sharded cache ([`lru`](https://docs.rs/lru) + [`parking_lot`](https://docs.rs/parking_lot)).
#[derive(Debug, Default)]
pub struct ShardedBlockCache {}
