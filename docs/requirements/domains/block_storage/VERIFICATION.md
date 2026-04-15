# Block Storage - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| BLK-001 | done | Store block with zstd compression, header, record cache, canonical mapping; idempotent | Integration: `tests/blk_001_tests.rs` — round-trip hash, idempotent CF_BYTES stable, zstd/bincode CF layout, get_record vs from_header, canonical on/off |
| BLK-002 | done | Get block by hash with cache, zstd dictionary fallback, bincode deserialization | Integration: `tests/blk_002_tests.rs` — physical get counter, read-through, dict + plain fallback reopen, unknown hash |
| BLK-003 | done | Get header by hash with cache, no decompression | Integration: `tests/blk_003_tests.rs` — physical get counter, read-through, raw CF_HEADERS bincode (no zstd), unknown hash |
| BLK-004 | done | Get record by hash, derived from header, never persisted | Integration: `tests/blk_004_tests.rs` — cache hit skips physical CF_HEADERS; header-warm miss; full miss read-through; reopen derives record; CF layout (header bincode / zstd blocks); unknown hash |
| BLK-005 | done | Batch retrieval with multi_get for cache misses | Integration: `tests/blk_005_tests.rs` — all hits skip batch counter; all misses one batch; mixed order; ordering permutation; missing slot None; read-through warms `get_block`; empty input |
| BLK-006 | done | Prefetch sequential blocks via readahead iterator on CF_CANONICAL | Integration: `tests/blk_006_tests.rs` — height order slice 10..50; readahead_size wiring; second pass cache-only; missing body BlockNotFound; inverted range empty; optional ignored throughput smoke |
| BLK-007 | gap | Async wrappers: cache hits non-blocking, misses via spawn_blocking | Unit: cache hit resolves without spawn_blocking; cache miss dispatches to spawn_blocking; results match sync equivalents |
| BLK-008 | gap | Write pipeline batches puts into WriteBatch via bounded mpsc channel | Integration: send 256 blocks, verify all stored; verify batching (batch_size and flush_ms); benchmark throughput vs serial put |
| BLK-009 | gap | Attestation put/get via bincode in CF_ATTESTED | Unit: round-trip put/get attestation; missing hash returns None; verify bincode serialization format |
| BLK-010 | gap | Update BlockRecord status in cache only, no disk write | Unit: update status on cached record; verify no RocksDB write; error on missing record |
| BLK-011 | gap | Has block existence check by hash | Unit: store a block, verify has_block returns true; verify false for unknown hash; verify no deserialization occurs |
| BLK-012 | gap | Storage statistics via stats() | Integration: store blocks, checkpoints, attestations; call stats(); verify all counts match; verify tip_height and min_height; verify total_size_bytes is non-zero |
| BLK-013 | gap | Flush WAL and trigger manual compaction | Integration: write blocks, call flush(), verify durability; call compact(), verify no error; verify RocksDB errors propagated |
| BLK-014 | gap | Get canonical blocks in height range (inclusive) | Unit: store canonical blocks 0-9, get_blocks_in_range(3,7) returns 5 blocks in order; empty range returns empty vec; verify ascending height order |
| BLK-015 | gap | Get canonical records in height range (inclusive) | Unit: store canonical blocks 0-9, get_records_in_range(3,7) returns 5 records in order; empty range returns empty vec; lighter than full block retrieval |
