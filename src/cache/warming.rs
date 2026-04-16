//! Cache warming on startup ([`CAC-006`](../../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
//!
//! Preloads recent canonical blocks into all in-memory caches during
//! [`crate::store::BlockStore::open`] when `warm_cache_on_open` is `true`.
//!
//! The warming strategy calls [`BlockStore::get_block_by_height`] and
//! [`BlockStore::get_record_by_height`] for each height from tip backward,
//! which auto-populates block_cache, header_cache, record_cache,
//! canonical_height_cache, and hash_to_height_cache through read-through paths.

use crate::store::BlockStore;

impl BlockStore {
    /// Populate ALL in-memory caches by reading the most recent canonical blocks.
    ///
    /// Walks backward from the chain tip for up to `depth` heights. At each height,
    /// calls `get_block_by_height` and `get_record_by_height` which auto-populate
    /// all caches through their read-through paths.
    ///
    /// Errors are silently skipped — `open()` always succeeds. Subsequent reads
    /// fill gaps on demand.
    ///
    /// Returns count of blocks successfully loaded into cache.
    pub(crate) fn warm_caches(&self, depth: u64) -> usize {
        let Some(t) = self.tip() else {
            return 0;
        };
        let start = t.height.saturating_sub(depth.saturating_sub(1));
        let mut count = 0usize;
        for h in (start..=t.height).rev() {
            if self.get_block_by_height(h).ok().flatten().is_some() {
                let _ = self.get_record_by_height(h);
                count += 1;
            }
        }
        count
    }
}
