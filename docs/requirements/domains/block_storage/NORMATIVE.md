# Block Storage - Normative Requirements

- **Domain:** block_storage
- **Prefix:** BLK
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### BLK-001: Store Block (`put`)

`put(&self, block: &L2Block, canonical: bool) -> Result<bool>`

1. MUST serialize the full block with zstd compression and write it to `CF_BLOCKS` keyed by block hash.
2. MUST serialize the block header with bincode (uncompressed) and write it to `CF_HEADERS` keyed by block hash.
3. MUST create a `BlockRecord` from the header via `from_header()` and insert it into the in-memory record cache.
4. If `canonical` is `true`, MUST write a height-to-hash mapping in `CF_CANONICAL`.
5. MUST be idempotent: if a block with the same hash already exists in `CF_BLOCKS`, return `Ok(false)` without overwriting.
6. MUST return `Ok(true)` when a new block is successfully stored.

**Spec reference:** 5.1

---

### BLK-002: Get Block by Hash (`get_block`)

`get_block(&self, hash: &Bytes32) -> Result<Option<L2Block>>`

1. MUST check the block cache first (O(1) lookup).
2. On cache miss, MUST read the compressed bytes from `CF_BLOCKS`.
3. MUST decompress using the zstd dictionary; MUST fall back to plain zstd decompression if dictionary decompression fails.
4. MUST deserialize the decompressed bytes with bincode.
5. MUST populate the block cache on successful read-through.
6. MUST return `Ok(None)` when no block exists for the given hash.

**Spec reference:** 5.2

---

### BLK-003: Get Header by Hash (`get_header`)

`get_header(&self, hash: &Bytes32) -> Result<Option<L2BlockHeader>>`

1. MUST check the header cache first (O(1) lookup).
2. On cache miss, MUST read from `CF_HEADERS` and deserialize with bincode.
3. Headers are stored uncompressed; MUST NOT attempt zstd decompression.
4. MUST populate the header cache on successful read-through.
5. MUST return `Ok(None)` when no header exists for the given hash.

**Spec reference:** 5.2

---

### BLK-004: Get Record by Hash (`get_record`)

`get_record(&self, hash: &Bytes32) -> Result<Option<BlockRecord>>`

1. MUST check the record cache first (O(1) lookup).
2. On cache miss, MUST read the header from `CF_HEADERS`, deserialize it, and derive a `BlockRecord` via `from_header()`.
3. MUST populate the record cache on successful derivation.
4. `BlockRecord` values are NEVER persisted to disk; they exist only in the in-memory cache.
5. MUST return `Ok(None)` when no header exists for the given hash.

**Spec reference:** 5.2

---

### BLK-005: Batch Retrieval (`get_blocks_by_hash`)

`get_blocks_by_hash(&self, hashes: &[Bytes32]) -> Result<Vec<Option<L2Block>>>`

1. MUST check the block cache for each requested hash.
2. For cache misses, MUST issue a single RocksDB `multi_get` call (one I/O round-trip).
3. MUST decompress and deserialize each hit from the `multi_get` result.
4. MUST populate the block cache for each successfully retrieved block.
5. MUST preserve the input ordering in the returned `Vec`.

**Spec reference:** 5.3

---

### BLK-006: Prefetching for Sequential Access (`stream_blocks_in_range`)

`stream_blocks_in_range(&self, start: u64, end: u64) -> impl Iterator<Item=Result<L2Block>>`

1. MUST use a readahead iterator on `CF_CANONICAL` to resolve height-to-hash mappings for the requested range.
2. MUST prefetch block data from `CF_BLOCKS` using RocksDB readahead.
3. SHOULD allow configurable `readahead_size` for tuning sequential scan performance.
4. MUST yield blocks in height order from `start` to `end` (inclusive).

**Spec reference:** 5.4

---

### BLK-007: Async API

`get_block_async`, `get_header_async`, `get_block_by_height_async`

1. MUST serve cache hits directly on the tokio executor without blocking.
2. MUST dispatch cache-miss RocksDB reads to `tokio::task::spawn_blocking` to avoid blocking the async runtime.
3. MUST return the same results as their synchronous counterparts.

**Spec reference:** 5.5

---

### BLK-008: Write Pipeline (`put_pipelined`)

`put_pipelined(block, canonical) -> Result<()>`

1. MUST accept blocks into a bounded `mpsc` channel.
2. MUST batch up to `write_pipeline_batch_size` blocks (default: 64) or flush after `write_pipeline_flush_ms` (default: 100 ms), whichever comes first.
3. MUST issue a single RocksDB `WriteBatch` for each accumulated batch.
4. SHOULD yield 5-10x throughput improvement over individual `put` calls during initial sync.

**Spec reference:** 5.1.1

---

### BLK-009: Attestation Storage

`put_attestation(&self, hash: &Bytes32, attested: &AttestedBlock) -> Result<()>`
`get_attestation(&self, hash: &Bytes32) -> Result<Option<AttestedBlock>>`

1. `put_attestation` MUST serialize the `AttestedBlock` via bincode and write it to `CF_ATTESTED` keyed by block hash.
2. `get_attestation` MUST read from `CF_ATTESTED`, deserialize with bincode, and return the result.
3. MUST return `Ok(None)` from `get_attestation` when no attestation exists for the given hash.

**Spec reference:** 5.6

---

### BLK-010: Status Updates (`update_status`)

`update_status(&self, hash: &Bytes32, status: BlockStatus) -> Result<()>`

1. MUST update the `BlockRecord` in the in-memory record cache with the new `BlockStatus`.
2. MUST NOT write status changes to disk; `BlockRecord` values are cache-only.
3. MUST return an error if no `BlockRecord` exists in the cache for the given hash.

**Spec reference:** 15.8

---

### BLK-011: Has Block (`has_block`)

`has_block(&self, hash: &Bytes32) -> Result<bool>`

1. MUST return `true` if a block with the given hash exists in the store (in `CF_HEADERS` or `CF_BLOCKS`).
2. MUST check the in-memory cache first; on cache miss, MUST perform a RocksDB key-existence check.
3. MUST NOT deserialize or decompress the block data -- existence check only.
4. MUST return `false` when no block with the given hash exists.

**Spec reference:** 15.2

---

### BLK-012: Storage Statistics (`stats`)

`stats(&self) -> Result<StorageStats>`

1. MUST return a `StorageStats` struct populated with current storage metrics.
2. `block_count` MUST reflect the total number of entries in `CF_BLOCKS` (all forks).
3. `canonical_block_count` MUST reflect the number of entries in `CF_CANONICAL`.
4. `header_count` MUST reflect the total number of entries in `CF_HEADERS`.
5. `checkpoint_count` MUST reflect the total number of entries in `CF_CHECKPOINTS`.
6. `attested_count` MUST reflect the total number of entries in `CF_ATTESTED`.
7. `tip_height` MUST be `Some(height)` if a chain tip is set, `None` otherwise.
8. `min_height` MUST be `Some(h)` if pruning has occurred, `None` if no pruning.
9. `total_size_bytes` MUST be an estimate of total disk usage across all column families.

**Spec reference:** 15.7

---

### BLK-013: Flush and Compact

`flush(&self) -> Result<()>` and `compact(&self) -> Result<()>`

1. `flush` MUST force a RocksDB WAL flush, ensuring all buffered writes are durable on disk.
2. `compact` MUST trigger a manual RocksDB compaction across all column families.
3. Both methods MUST propagate any RocksDB errors via `BlockStoreError::RocksDb`.

**Spec reference:** 15.7

---

### BLK-014: Get Blocks in Range (`get_blocks_in_range`)

`get_blocks_in_range(&self, start_height: u64, end_height: u64) -> Result<Vec<L2Block>>`

1. MUST return canonical blocks in the height range `[start_height, end_height]` (inclusive on both ends).
2. MUST return blocks in ascending height order.
3. MUST return an empty `Vec` if `start_height > end_height`.
4. MUST skip heights that have no canonical entry (gaps in canonical index).
5. MUST validate that requested heights are within the stored range.

**Spec reference:** 5.3, 15.3

---

### BLK-015: Get Records in Range (`get_records_in_range`)

`get_records_in_range(&self, start_height: u64, end_height: u64) -> Result<Vec<BlockRecord>>`

1. MUST return canonical block records in the height range `[start_height, end_height]` (inclusive).
2. MUST return records in ascending height order.
3. MUST return an empty `Vec` if `start_height > end_height`.
4. Lighter than `get_blocks_in_range` -- no full block deserialization or decompression required.
5. Records MUST be derived from headers on cache miss via `from_header()`.

**Spec reference:** 5.3, 15.3
