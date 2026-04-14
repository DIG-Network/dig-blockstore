//! Cache warming on startup ([`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
//!
//! Preloads recent canonical blocks into [`super::sharded::ShardedBlockCache`] after open.

/// Controls optional preload of recent heights ([`BlockStore::open`](crate::store::BlockStore) will invoke later).
#[derive(Debug, Default)]
pub struct CacheWarming {}
