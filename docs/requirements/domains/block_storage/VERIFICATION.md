# Block Storage - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| BLK-001 | done | Store block with zstd compression, header, record cache, canonical mapping; idempotent | Integration: `tests/blk_001_tests.rs` — round-trip hash, idempotent CF_BYTES stable, zstd/bincode CF layout, get_record vs from_header, canonical on/off |
| BLK-002 | done | Get block by hash with cache, zstd dictionary fallback, bincode deserialization | Integration: `tests/blk_002_tests.rs` — physical get counter, read-through, dict + plain fallback reopen, unknown hash |
| BLK-003 | gap | Get header by hash with cache, no decompression | Unit: cache hit path; cache miss read-through from CF_HEADERS; verify no decompression attempted; missing hash returns None |
| BLK-004 | gap | Get record by hash, derived from header, never persisted | Unit: cache hit path; cache miss derives from header via from_header(); verify record never written to disk; missing hash returns None |
| BLK-005 | gap | Batch retrieval with multi_get for cache misses | Unit: all-cache-hit path; all-miss path uses single multi_get; mixed hit/miss; verify ordering preserved; verify cache populated for misses |
| BLK-006 | gap | Prefetch sequential blocks via readahead iterator on CF_CANONICAL | Integration: store range of canonical blocks, stream and verify order; benchmark readahead vs individual reads; verify configurable readahead_size |
| BLK-007 | gap | Async wrappers: cache hits non-blocking, misses via spawn_blocking | Unit: cache hit resolves without spawn_blocking; cache miss dispatches to spawn_blocking; results match sync equivalents |
| BLK-008 | gap | Write pipeline batches puts into WriteBatch via bounded mpsc channel | Integration: send 256 blocks, verify all stored; verify batching (batch_size and flush_ms); benchmark throughput vs serial put |
| BLK-009 | gap | Attestation put/get via bincode in CF_ATTESTED | Unit: round-trip put/get attestation; missing hash returns None; verify bincode serialization format |
| BLK-010 | gap | Update BlockRecord status in cache only, no disk write | Unit: update status on cached record; verify no RocksDB write; error on missing record |
| BLK-011 | gap | Has block existence check by hash | Unit: store a block, verify has_block returns true; verify false for unknown hash; verify no deserialization occurs |
| BLK-012 | gap | Storage statistics via stats() | Integration: store blocks, checkpoints, attestations; call stats(); verify all counts match; verify tip_height and min_height; verify total_size_bytes is non-zero |
| BLK-013 | gap | Flush WAL and trigger manual compaction | Integration: write blocks, call flush(), verify durability; call compact(), verify no error; verify RocksDB errors propagated |
| BLK-014 | gap | Get canonical blocks in height range (inclusive) | Unit: store canonical blocks 0-9, get_blocks_in_range(3,7) returns 5 blocks in order; empty range returns empty vec; verify ascending height order |
| BLK-015 | gap | Get canonical records in height range (inclusive) | Unit: store canonical blocks 0-9, get_records_in_range(3,7) returns 5 records in order; empty range returns empty vec; lighter than full block retrieval |
