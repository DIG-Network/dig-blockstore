# Caching - Normative Requirements

- **Domain:** caching
- **Prefix:** CAC
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### CAC-001: Sharded Block Cache

1. The block cache MUST use 16 shards by default (configurable via `cache_shards`).
2. Each shard MUST be an independent LRU cache with capacity equal to `block_cache_capacity / cache_shards`.
3. Shard selection MUST be determined by `hash[0] % num_shards` where `hash` is the block's `Bytes32` key.
4. Each shard MUST be protected by an independent `parking_lot::RwLock` to reduce lock contention under concurrent reads.

**Spec reference:** 11.1

---

### CAC-002: Sharded Header Cache

1. The header cache MUST use the same sharding strategy as the block cache (shard count, shard selection, per-shard `RwLock`).
2. The default header cache capacity MUST be 2000.
3. The header cache MUST be separate from the block cache (different access frequency patterns).

**Spec reference:** 11.2

---

### CAC-003: BlockRecord Cache

1. The `BlockRecord` cache MUST be in-memory only and MUST NOT be persisted to disk.
2. On cache miss, a `BlockRecord` MUST be derived from the header via `from_header()`.
3. The cache MUST be updated on `put_block` (new record inserted).
4. The cache MUST be updated on `update_status` (status field changed).
5. The cache MUST be updated on `set_canonical` (the `in_canonical_chain` flag changed).

**Spec reference:** 11.3

---

### CAC-004: Canonical Height Index Cache

1. The canonical height index SHOULD maintain an in-memory `BTreeMap<u64, Bytes32>` mapping heights to canonical block hashes.
2. The cache SHOULD be populated from mmap reads of `CF_CANONICAL`.
3. The cache MUST be evicted (entries removed) on rollback to maintain consistency.
4. The cache MUST provide O(log n) lookups for hot heights without requiring mmap access.

**Spec reference:** 11.4

---

### CAC-005: Hash-to-Height Reverse Cache

1. The hash-to-height reverse lookup cache SHOULD map `Bytes32` to `u64` for recently accessed blocks.
2. The cache MUST be used by `find_common_ancestor` and `set_canonical` to avoid header deserialization for height lookups.
3. The cache MUST be updated when blocks are stored or headers are read.

**Spec reference:** 11.5

---

### CAC-006: Cache Warming on Startup

1. When `warm_cache_on_open` is `true`, `BlockStore::open()` MUST preload the most recent N blocks and headers into caches, where N equals `block_cache_capacity`.
2. Cache warming MUST read the canonical chain backwards from the tip.
3. Cache warming SHOULD complete before `BlockStore::open()` returns.

**Spec reference:** 11.6
