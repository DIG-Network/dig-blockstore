//! Cache warming on startup ([`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
//!
//! Warming logic lives in [`crate::store::BlockStoreInner::warm_caches`], called from
//! [`crate::store::BlockStore::open`] when `warm_cache_on_open` is `true`.
