# Caching - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| CAC-001 | gap | Sharded block cache with 16 default shards, LRU per shard, shard by hash[0] % num_shards, parking_lot::RwLock per shard | Unit: verify 16 shards created by default; verify shard selection by first byte; verify per-shard capacity = total/shards; concurrent read stress test for lock contention |
| CAC-002 | gap | Sharded header cache, same strategy as block cache, default capacity 2000, separate instance | Unit: verify default capacity 2000; verify shard selection matches block cache strategy; verify header and block caches are independent |
| CAC-003 | gap | BlockRecord cache: in-memory only, derived from header on miss, updated on put_block/update_status/set_canonical | Unit: verify cache miss triggers from_header(); verify put_block inserts record; verify update_status updates record; verify set_canonical updates in_canonical_chain flag; verify no disk persistence |
| CAC-004 | gap | Canonical height index BTreeMap<u64, Bytes32> for O(log n) lookups; populated from mmap; evicted on rollback | Unit: verify O(log n) lookup; verify population from canonical chain; verify rollback evicts entries; verify no stale data after eviction |
| CAC-005 | gap | Hash-to-height reverse cache Bytes32 -> u64; used by find_common_ancestor and set_canonical to avoid header deserialization | Unit: verify cache populated on block store/header read; verify find_common_ancestor uses cache; verify set_canonical uses cache; verify miss falls through to header lookup |
| CAC-006 | gap | Cache warming on open: preload N blocks/headers from canonical tip backwards when warm_cache_on_open is true | Integration: store 100 blocks; open with warm_cache_on_open=true; verify most recent N blocks in cache without explicit reads; verify warm_cache_on_open=false skips warming |
