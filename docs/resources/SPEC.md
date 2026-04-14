# dig-blockstore Specification

**Version:** 0.1.0
**Status:** Draft
**Date:** 2026-04-14

## 1. Overview

`dig-blockstore` is a self-contained Rust crate that owns one concern for the DIG Network L2 blockchain: **persistent storage and retrieval of blocks, headers, attestations, and checkpoints**. It provides a RocksDB-backed store with in-memory caching, canonical chain tracking, fork management, rollback support, and pruning — all behind a clean trait-based API. The crate stores already-validated blocks; it never executes CLVM, never validates signatures, and never makes network calls.

The design is informed by two sources:
- **DIG L2 driver** (`l2_driver_state_channel`) — the current monolithic implementation being decomposed into this crate.
- **Chia L1 blockchain** (`chia-blockchain`) — Chia's production block store patterns: separation of full blocks from lightweight block records, dense height-to-hash mappings, `in_main_chain` tracking, zstd compression, LRU caching, idempotent writes, and concurrent reader/writer access.

The crate **does** own:
- **Block persistence** — Storing `L2Block` and `AttestedBlock` (from `dig-block`) keyed by block hash in RocksDB, with zstd compression for full block payloads.
- **Header persistence** — Storing `L2BlockHeader` separately from full blocks for lightweight queries (header-only sync, metadata extraction) without deserializing the full block body.
- **Block metadata** — A lightweight `BlockRecord` extracted from the header at insertion time, stored separately for fast queries without deserializing the header. Adopted from Chia's pattern of separating `BlockRecord` from `FullBlock`.
- **Canonical chain mapping** — A dense `height → hash` forward index that identifies which block is canonical at each height. Adopted from Chia's `in_main_chain` flag and `BlockHeightMap` pattern.
- **Chain tip tracking** — Maintaining the current chain tip (peak) as the highest canonical block, with atomic updates on block insertion and rollback.
- **Checkpoint persistence** — Storing finalized `Checkpoint` and `StoredCheckpoint` (checkpoint + attestation metadata) keyed by epoch number.
- **Fork storage** — Non-canonical blocks are stored alongside canonical blocks (same column family) but are not referenced by the height index. This allows fork blocks to be retrieved by hash for reorg evaluation without polluting the canonical chain view.
- **Range queries** — Efficient retrieval of blocks by height range, epoch, or hash batch, using RocksDB iterators and the canonical height index.
- **Rollback** — Reverting the canonical chain to a prior height by updating the height index and tip, without deleting the now-orphaned blocks (they remain retrievable by hash for potential future reorg).
- **Pruning** — Removing blocks, headers, and metadata below a configurable height threshold to bound storage growth.
- **Caching** — Sharded in-memory LRU caches for recently accessed blocks and a BTreeMap-backed height index for O(log n) canonical lookups without touching RocksDB. Cache warming on startup preloads recent blocks.
- **Write pipeline** — Asynchronous write channel that batches multiple blocks into a single RocksDB `WriteBatch` for high-throughput ingestion during initial sync.
- **Async API** — First-class async methods that serve cache hits on the tokio executor and dispatch cache-miss RocksDB reads to a blocking threadpool.
- **Snapshot export/import** — Streaming export of canonical block ranges for fast sync, enabling new nodes to bootstrap from a checkpoint instead of replaying from genesis.
- **Storage statistics** — Block count, total size, height range, and per-column-family metrics.
- **Error types** — `BlockStoreError` covering I/O failures, missing blocks, serialization errors, and constraint violations.

The crate does **not** own:
- **Block types** (L2BlockHeader, L2Block, AttestedBlock, Checkpoint, CheckpointSubmission, BlockStatus) — owned by `dig-block`. This crate stores instances of these types but does not define them.
- **Block validation** (structural, execution, or state validation) — owned by `dig-block`. Blocks are validated before being passed to the store.
- **Block production** (BlockBuilder, transaction selection) — owned by `dig-block` and the proposer layer.
- **Global coin state** (UTXO set, state root computation, coin queries) — owned by `dig-coinstore`.
- **Epoch lifecycle** (phase management, checkpoint competition, reward distribution) — owned by `dig-epoch`. This crate stores epoch artifacts (checkpoints, summaries) but does not manage epoch state machines.
- **Fork choice policy** (which fork to follow, weight comparison, finality rules) — owned by the consensus layer. This crate executes rollback and tip updates when told to by the consensus layer, but does not decide which fork wins.
- **Transaction pool** — owned by `dig-mempool`.
- **Networking** (block gossip, peer sync) — owned by `dig-gossip`.
- **CLVM execution** — owned by `dig-clvm`.

**Hard boundary:** The crate operates as a **keyed block repository with canonical chain indexing**. It accepts already-validated blocks, persists them, maintains an authoritative height-to-hash mapping for the canonical chain, and serves queries. External decisions (which block is canonical, when to rollback, what to prune) are made by callers and executed through the store's API. The crate never validates block content, never resolves forks, and never initiates I/O beyond its local RocksDB instance.

### 1.1 Design Principles

- **Store validated blocks, query fast**: Blocks arrive already validated by `dig-block`. The store's job is to persist them durably and serve them back efficiently. Every read path is optimized: cache first, then index, then disk.
- **Separate concerns by access pattern**: Full blocks (large, infrequent access), headers (medium, frequent access), and block records/metadata (small, very frequent access) live in separate column families with tuned RocksDB settings. Adopted from Chia's deliberate separation of `FullBlock` from `BlockRecord` for parse-speed optimization.
- **Canonical chain is an index, not a copy**: The canonical chain is a `height → hash` mapping in its own column family. Blocks themselves are stored once by hash. Switching the canonical chain (reorg) updates the index, not the blocks. Adopted from Chia's `in_main_chain` flag pattern, but implemented as a separate index for RocksDB efficiency.
- **Forks are kept, not deleted**: Non-canonical blocks remain in the store, retrievable by hash. Only the canonical index is updated during reorg. This enables efficient reorg (no re-download) and historical analysis. Pruning is a separate, explicit operation.
- **Compression for payloads, not keys**: Full block bodies are zstd-compressed before writing to RocksDB (adopted from Chia's `zstd.compress(bytes(block))`). Headers, metadata, and index entries are stored uncompressed for fast point-lookup deserialization.
- **Idempotent writes**: Inserting a block that already exists (same hash) is a no-op, not an error. Adopted from Chia's `INSERT OR IGNORE` pattern. This simplifies retry logic and parallel block processing.
- **Dense height index with mmap fast-path**: The canonical height mapping uses a memory-mapped file (`canonical.bin`) for O(1) lookups with zero syscall overhead on the hot path. RocksDB `CF_CANONICAL` serves as the durable backup, rebuilt from the mmap file on startup. Adopted from Chia's dense `bytearray` height-to-hash mapping, upgraded to use OS page cache directly.
- **Write pipeline for sync throughput**: During initial sync, blocks are accepted into a bounded async channel and batched into RocksDB `WriteBatch` operations (50-100 blocks per batch). This amortizes fsync and WAL overhead across many blocks, yielding 5-10x throughput improvement over single-block writes.
- **Dictionary-trained zstd compression**: Block bodies are compressed with a pre-trained zstd dictionary (trained on a sample of ~1000 blocks). Since L2 blocks have highly repetitive structure, dictionary compression improves ratio by 20-40% over plain zstd.
- **BlobDB for large block values**: Full block bodies in `CF_BLOCKS` use RocksDB's BlobDB to store large values in separate blob files, keeping the LSM tree lean and compaction fast.
- **Maximal reuse of Chia and DIG crates**: Block types come from `dig-block`. `Bytes32` comes from `chia-protocol`. Serialization uses bincode (matching `dig-block`). The `Streamable` trait from `chia-traits` is supported for wire-format interop. SHA-256 uses `chia-sha2`. No custom reimplementations of types or algorithms that existing crates already provide.

### 1.2 Crate Dependencies

The crate maximally reuses the Chia Rust ecosystem and DIG crates to avoid reimplementing production-hardened primitives. The principle is: **if a Chia or DIG crate already provides it, use it — don't rewrite it.**

| Crate | Version | Purpose |
|-------|---------|---------|
| `dig-block` | 0.1 | All block types: `L2BlockHeader`, `L2Block`, `AttestedBlock`, `Checkpoint`, `CheckpointSubmission`, `BlockStatus`, `SignerBitmap`, `ReceiptList`. The authoritative source of block definitions — `dig-blockstore` stores these, never redefines them. |
| `dig-epoch` | 0.1 | `EpochSummary`, epoch height arithmetic (`epoch_for_block_height`, `epoch_checkpoint_height`, `is_checkpoint_class_block`). Used to derive epoch-based indexes and validate checkpoint storage keys. |
| `dig-constants` | 0.1 | Network-level constants: `NetworkConstants`, network ID. |
| `chia-protocol` | 0.26 | `Bytes32` — the universal 32-byte hash type used for all keys (block hashes, coin IDs, Merkle roots). `Coin` referenced transitively through `dig-block`. |
| `chia-bls` | 0.26 | `Signature`, `PublicKey` — referenced transitively through `dig-block`'s `AttestedBlock` and `CheckpointSubmission`. Stored as-is in serialized blocks. |
| `chia-sha2` | 0.26 | `Sha256` — used for computing block record digests and verifying block hash consistency on read-back. Same SHA-256 implementation used by `dig-block::L2BlockHeader::hash()`. |
| `chia-traits` | 0.26 | `Streamable` trait — used for wire-format serialization of blocks when serving them to peers via `dig-gossip`. Blocks are stored in bincode but can be exported as Streamable for protocol interop. |
| `rocksdb` | — | Persistent key-value storage backend. Column families provide logical separation with per-CF tuning (bloom filters, compression, block cache). |
| `zstd` | — | Zstandard compression for full block payloads. Adopted from Chia's block compression pattern. Typical 3-5x compression ratio on block bodies. |
| `bincode` | — | Compact binary serialization for all stored types. Matches `dig-block`'s serialization format — blocks can be stored as-is from `to_bytes()`. |
| `serde` | — | Serialization/deserialization framework. All stored types derive `Serialize` + `Deserialize`. |
| `thiserror` | — | Error type derivation for `BlockStoreError`. |
| `parking_lot` | — | `RwLock` for concurrent read access to in-memory caches and indexes. |
| `lru` | — | LRU cache implementation for block and header caches. |
| `tokio` | — | Async runtime for the async API layer and write pipeline channel. `spawn_blocking()` for RocksDB reads on cache miss. |
| `memmap2` | — | Memory-mapped canonical height index (`canonical.bin`). O(1) height-to-hash lookups via OS page cache. |

**Key types used from the Chia ecosystem:**

| Type | From Crate | Usage in dig-blockstore |
|------|-----------|------------------------|
| `Bytes32` | chia-protocol | All RocksDB keys (block hashes, 32 raw bytes). All hash values in `BlockRecord` metadata. |
| `Signature` | chia-bls | Stored inside `AttestedBlock` and `StoredCheckpoint` payloads (serialized via bincode, not interpreted). |
| `PublicKey` | chia-bls | Stored inside `StoredCheckpoint` aggregate pubkey (serialized, not interpreted). |
| `Sha256` | chia-sha2 | Block hash verification on read-back: `assert_eq!(header.hash(), expected_hash)`. Digest computation for `BlockRecord`. |
| `Streamable` | chia-traits | Wire-format export for peer serving: `block.to_streamable_bytes()` for gossip protocol compatibility. |
| `L2BlockHeader` | dig-block | Stored in `CF_HEADERS` for lightweight queries. |
| `L2Block` | dig-block | Stored zstd-compressed in `CF_BLOCKS` as the full block payload. |
| `AttestedBlock` | dig-block | Stored in `CF_ATTESTED` after validator attestation. |
| `BlockStatus` | dig-block | Stored in `BlockRecord` metadata, updated as block progresses through validation/finalization. |
| `Checkpoint` | dig-block | Stored in `CF_CHECKPOINTS` keyed by epoch. |

### 1.3 Design Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | RocksDB, not SQLite or LMDB | RocksDB provides column families (logical separation with per-CF tuning), native prefix iteration (efficient range scans by height), built-in bloom filters (fast negative lookups), and native compression. Chia uses SQLite (good for their query patterns), but DIG's access patterns (hash-keyed lookups, range scans, write-heavy block ingestion) favor an LSM-tree store. `dig-coinstore` already uses RocksDB in the DIG ecosystem. |
| 2 | Separate column families per access pattern | Full blocks (large, sequential write, infrequent read) get different RocksDB tuning than headers (medium, frequent point-read) and metadata (small, very frequent point-read). Bloom filters on lookup-heavy CFs, none on sequential CFs. Adopted from the L2 driver's 9-CF design and Chia's observation that most queries need only BlockRecord, not FullBlock. |
| 3 | Zstd compression for full blocks only | Full block bodies are 3-5x compressible. Headers and metadata are small and frequently read — compression overhead isn't worth it. Adopted from Chia's `zstd.compress(bytes(block))` pattern. |
| 4 | `BlockRecord` extracted at insert time | A lightweight metadata struct is derived from the header at `put()` time and stored separately. Queries that only need height, epoch, parent hash, fees, or status never deserialize the full header or block. Adopted from Chia's deliberate duplication of BlockRecord alongside FullBlock for parse-speed optimization. |
| 5 | Height index as a separate column family | `CF_CANONICAL` maps `height (u64 BE) → hash (32 bytes)`. This is cheaper to update during reorg than Chia's `UPDATE SET in_main_chain=0` pattern because only the index entries change, not the block rows. Rollback is a range-delete on the index. |
| 6 | Big-endian u64 keys for natural sort order | RocksDB sorts keys lexicographically. Big-endian u64 encoding ensures height 1 < height 2 < ... < height N in iteration order, enabling efficient range scans without a custom comparator. |
| 7 | Fork blocks stored, not deleted | Non-canonical blocks remain in `CF_BLOCKS` and `CF_HEADERS`. Only the canonical index is updated. This avoids re-downloading blocks during reorg and enables historical fork analysis. Pruning is separate and explicit. Adopted from Chia's pattern of keeping orphan blocks with `in_main_chain=0`. |
| 8 | Idempotent `put()` | Inserting a block with an existing hash is a silent no-op. This simplifies gossip deduplication, parallel processing, and retry logic. Adopted from Chia's `INSERT OR IGNORE`. |
| 9 | LRU cache with configurable capacity | Recent blocks are cached in memory (default 1000). Cache is checked before RocksDB on every read. Adopted from Chia's `LRUCache[bytes32, FullBlock]` with capacity 1000. |
| 10 | Tip is a metadata entry, not computed | The chain tip (hash, height) is stored in `CF_METADATA` and updated atomically with each canonical chain extension or rollback. Querying the tip is O(1), not a scan. Adopted from Chia's `current_peak` single-row table. |
| 11 | Atomic batch writes | Block insertion (block + header + record + canonical entry + tip update) uses a RocksDB `WriteBatch` for atomicity. Either all writes succeed or none do. Prevents partial state on crash. |
| 12 | Maximal reuse of Chia and DIG crates — no custom type definitions | All block types come from `dig-block`. `Bytes32` from `chia-protocol`. SHA-256 from `chia-sha2`. Streamable interop from `chia-traits`. Epoch arithmetic from `dig-epoch`. The only new types are storage-specific: `BlockRecord` (metadata), `StoredCheckpoint`, `BlockStoreConfig`, `BlockStoreError`, and `StorageStats`. |
| 13 | `BlockRecord` is in-memory only, derived from headers on the fly | Chia stores `BlockRecord` in a separate DB column alongside `FullBlock`. But DIG's `L2BlockHeader` is a flat struct — bincode deserialization is sub-microsecond. Eliminating a separate CF_RECORDS avoids one write per block and the consistency burden of keeping records in sync with headers. Records are cached in memory and derived from `CF_HEADERS` on cache miss. If profiling reveals header deserialization as a bottleneck, CF_RECORDS can be reintroduced. |
| 14 | Memory-mapped canonical index | `CF_CANONICAL` is the durable backup, but the hot read path uses a memory-mapped file (`canonical.bin`) — a dense array of 32-byte hashes at `offset = height × 32`. Lookup is a pointer dereference into the OS page cache, not a RocksDB read. Rollback is `truncate()`. This is the single hottest path in the store (called on every block during sync, validation, and serving). |
| 15 | Write pipeline with async channel | Single-block `put()` writes are fine at 3s block time, but initial sync ingests thousands of blocks per second. A bounded `tokio::sync::mpsc` channel accepts blocks, and a background task batches them into RocksDB `WriteBatch` operations. Configurable batch size (default 64) and flush interval (default 100ms). |
| 16 | Sharded LRU caches | A single `RwLock<LruCache>` becomes a contention bottleneck under concurrent RPC load. Sharding the cache into 16 segments (keyed by first byte of hash, which is already well-distributed) reduces lock contention by ~16x. |
| 17 | BlobDB for CF_BLOCKS | Full block bodies average 1-5KB compressed. When inline in the LSM tree, they inflate SST files and make compaction expensive. RocksDB BlobDB stores values exceeding 512 bytes in separate blob files, keeping the LSM tree lean (keys + small pointers only). Compaction speed improves significantly as the chain grows. |
| 18 | Zstd dictionary compression | Block bodies have highly repetitive structure (same field layout, similar SpendBundle shapes). A pre-trained zstd dictionary (trained on ~1000 sample blocks, stored in CF_METADATA) improves compression ratio by 20-40% over plain zstd. The dictionary is ~100KB, loaded once at startup. |
| 19 | Compaction filter for automatic pruning | In addition to explicit `prune_before_height()`, a RocksDB compaction filter drops entries below the configured minimum height during background compaction. This amortizes pruning I/O across normal compaction cycles rather than requiring a separate pruning pass. |
| 20 | Per-CF compaction strategy | Different CFs have fundamentally different access patterns. `CF_BLOCKS` uses Universal compaction (write-optimized, large values). `CF_HEADERS` uses Level compaction (read-optimized). `CF_CANONICAL` uses a memory-mapped file (no compaction). `CF_CHECKPOINTS` uses Level compaction with large target file size. One-size-fits-all tuning leaves performance on the table. |

### 1.4 Chia Block Store Analysis

The DIG block store design draws on patterns from Chia's production block store implementation (`chia/full_node/block_store.py`). This section documents which patterns were adopted and which were adapted.

#### 1.4.1 Adopted Patterns

| # | Chia Pattern | Chia Source | DIG Adaptation |
|---|-------------|-------------|----------------|
| 1 | Separate FullBlock and BlockRecord storage | `full_blocks` table stores both `block` (compressed) and `block_record` columns | `CF_BLOCKS` (compressed full block) and `CF_RECORDS` (lightweight metadata) in separate column families. |
| 2 | Zstd compression for full blocks | `compress(block: FullBlock)` using zstd library | Same: `zstd::encode_all()` before writing to `CF_BLOCKS`. |
| 3 | LRU cache for recent blocks | `block_cache: LRUCache[bytes32, FullBlock]` with capacity 1000 | Same: `lru::LruCache<Bytes32, L2Block>` with configurable capacity (default 1000). |
| 4 | Single-row peak tracking | `current_peak` table with key=0, hash=peak_header_hash | `CF_METADATA` entry with key `"tip"` storing (hash, height). |
| 5 | Canonical chain tracking | `in_main_chain` boolean flag per block row | `CF_CANONICAL` column family: `height → hash`. Equivalent semantics, but more RocksDB-efficient. |
| 6 | Idempotent block insertion | `INSERT OR IGNORE INTO full_blocks` | `put()` checks existence before write, returns `Ok(false)` for duplicates. |
| 7 | Height-to-hash dense mapping | `BlockHeightMap` with bytearray (32 bytes per height) | `CF_CANONICAL` with big-endian u64 keys for natural sort order. |
| 8 | Partial indexes for main chain | `CREATE INDEX main_chain ON full_blocks(height, in_main_chain) WHERE in_main_chain=1` | `CF_CANONICAL` is inherently a "partial index" — only canonical blocks have height entries. |
| 9 | Reader/writer separation | `DBWrapper2` with multiple read connections and single write connection | `parking_lot::RwLock` on caches; RocksDB handles concurrent reads natively. |
| 10 | Block record caching in memory | `__block_records: dict[bytes32, BlockRecord]` in Blockchain class | `BTreeMap<u64, BlockRecord>` for recent heights, `HashMap<Bytes32, u64>` for hash-to-height. |

#### 1.4.2 Patterns Not Adopted (with rationale)

| # | Chia Pattern | Why Not Adopted |
|---|-------------|-----------------|
| 1 | SQLite as storage backend | DIG's access patterns (hash-keyed lookups, sequential writes, range scans) favor RocksDB's LSM-tree over SQLite's B-tree. RocksDB also provides native column families, bloom filters, and compression per-CF. |
| 2 | Sub-epoch segment storage | Chia stores `SubEpochChallengeSegments` for weight proofs (proof-of-space consensus). DIG uses epoch-based BLS finality — no equivalent segments needed. |
| 3 | Compactification tracking | Chia tracks `is_fully_compactified` for proof compression. DIG L2 does not have proof compactification. |
| 4 | Generator storage and retrieval | Chia stores CLVM generator programs separately (`get_generator()`, `get_block_info()`). DIG stores SpendBundles directly in the block body — no separate generator extraction needed. |
| 5 | Height-to-hash as file-backed bytearray | Chia persists `BlockHeightMap` to a separate file (`height-to-hash-{network}`). DIG uses a RocksDB column family (`CF_CANONICAL`) which is already persistent and crash-safe. |
| 6 | Weight-based fork choice in store | Chia's `_reconsider_peak()` compares block weights inside the blockchain class. DIG separates fork choice (consensus layer) from storage (this crate). |

## 2. Constants

### 2.1 Column Families

```rust
/// Full L2Block payloads, zstd-compressed. Keyed by block hash.
pub const CF_BLOCKS: &str = "blocks";

/// L2BlockHeader (uncompressed). Keyed by block hash.
/// Stored separately for lightweight header-only queries.
pub const CF_HEADERS: &str = "headers";

/// AttestedBlock data (signer bitmap, aggregate sig, receipts, status).
/// Keyed by block hash. Stored separately from L2Block because
/// attestation arrives after the block.
pub const CF_ATTESTED: &str = "attested";

/// Canonical chain index (durable backup): height → block hash.
/// Big-endian u64 keys for natural sort order.
/// The hot read path uses a memory-mapped file (canonical.bin);
/// CF_CANONICAL is the crash-recovery source of truth.
pub const CF_CANONICAL: &str = "canonical";

/// Finalized checkpoints. Keyed by epoch number (big-endian u64).
pub const CF_CHECKPOINTS: &str = "checkpoints";

/// Generic metadata (chain tip, config, storage stats).
/// Keyed by UTF-8 string names.
pub const CF_METADATA: &str = "metadata";
```

### 2.2 Metadata Keys

```rust
/// Chain tip: stores (hash: Bytes32, height: u64) = 40 bytes.
pub const META_TIP: &str = "tip";

/// Genesis block hash: stores Bytes32 = 32 bytes.
pub const META_GENESIS_HASH: &str = "genesis_hash";

/// Lowest stored height (after pruning): stores u64 = 8 bytes.
pub const META_MIN_HEIGHT: &str = "min_height";

/// Storage schema version: stores u32 = 4 bytes.
pub const META_SCHEMA_VERSION: &str = "schema_version";

/// Pre-trained zstd compression dictionary (~100KB).
/// Trained on a sample of ~1000 blocks for 20-40% better compression.
pub const META_ZSTD_DICT: &str = "zstd_dict";

/// Current schema version.
pub const SCHEMA_VERSION: u32 = 1;
```

### 2.3 RocksDB Tuning Defaults

```rust
/// Default write buffer size (64 MB).
pub const DEFAULT_WRITE_BUFFER_SIZE: usize = 64 * 1024 * 1024;

/// Default block cache size (128 MB shared across all CFs).
pub const DEFAULT_BLOCK_CACHE_SIZE: usize = 128 * 1024 * 1024;

/// Default max open files.
pub const DEFAULT_MAX_OPEN_FILES: i32 = 1000;

/// Default bloom filter bits per key (for lookup-heavy CFs).
pub const DEFAULT_BLOOM_BITS_PER_KEY: i32 = 10;

/// Default in-memory block cache capacity (number of blocks).
pub const DEFAULT_BLOCK_CACHE_CAPACITY: usize = 1000;

/// Default in-memory header cache capacity.
pub const DEFAULT_HEADER_CACHE_CAPACITY: usize = 2000;

/// Zstd compression level for full blocks.
pub const ZSTD_COMPRESSION_LEVEL: i32 = 3;
```

### 2.4 Per-CF Configuration

| Column Family | Bloom Filter | Compression | Compaction Style | BlobDB | Access Pattern |
|---------------|-------------|-------------|-----------------|--------|----------------|
| `CF_BLOCKS` | No | zstd dictionary (application-level) | Universal | Yes (min_blob_size=512) | Write-heavy, infrequent reads, large values |
| `CF_HEADERS` | Yes (10 bits) | None | Level | No | Frequent point-lookups, medium values |
| `CF_ATTESTED` | Yes (10 bits) | None | Level | No | Point-lookups, medium values |
| `CF_CANONICAL` | No | None | Level | No | Durable backup for mmap file; range scans on recovery |
| `CF_CHECKPOINTS` | No | None | Level (large target) | No | Infrequent, sequential by epoch |
| `CF_METADATA` | No | None | Level | No | Rare point-lookups, tiny values |

**BlobDB configuration for CF_BLOCKS:**

```rust
cf_opts.set_enable_blob_files(true);
cf_opts.set_min_blob_size(512);            // values > 512B → blob file
cf_opts.set_blob_file_size(256 * 1024 * 1024); // 256MB blob files
cf_opts.set_enable_blob_garbage_collection(true);
cf_opts.set_blob_garbage_collection_age_cutoff(0.25);
```

**Universal compaction for CF_BLOCKS** keeps write amplification low for the write-heaviest column family. Level compaction is used for read-heavy CFs where space amplification matters more.

## 3. Data Model

### 3.1 Primitive Types

| Type | Definition | Usage |
|------|-----------|-------|
| `Bytes32` | `[u8; 32]` (from `chia-protocol`) | Block hashes, Merkle roots — all RocksDB keys for hash-indexed CFs. |
| `L2BlockHeader` | Block header (from `dig-block`) | Stored in `CF_HEADERS`. |
| `L2Block` | Full block (from `dig-block`) | Stored zstd-compressed in `CF_BLOCKS`. |
| `AttestedBlock` | Block + attestation (from `dig-block`) | Stored in `CF_ATTESTED`. |
| `BlockStatus` | Validation/finality state (from `dig-block`) | Stored in `BlockRecord`. |
| `Checkpoint` | Epoch summary (from `dig-block`) | Stored in `CF_CHECKPOINTS`. |
| `CheckpointSubmission` | Signed checkpoint (from `dig-block`) | Stored inside `StoredCheckpoint`. |
| `SignerBitmap` | Validator bitmap (from `dig-block`) | Stored inside `StoredCheckpoint`. |

### 3.2 BlockRecord

Lightweight metadata derived from `L2BlockHeader` at insertion time. **Cached in memory only** — not persisted to a separate column family. On cache miss, the record is re-derived by deserializing the header from `CF_HEADERS` (sub-microsecond for DIG's flat header struct).

Adopted from Chia's `BlockRecord` concept but simplified: Chia persists `BlockRecord` alongside `FullBlock` because Chia's nested header parsing is slow. DIG's `L2BlockHeader` is a flat struct — bincode deserialization is fast enough to derive the record on the fly, eliminating one write per block and the consistency burden of a separate CF.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    // ── Identity ──
    pub hash: Bytes32,               // Block header hash
    pub height: u64,                 // Block height
    pub epoch: u64,                  // Epoch number
    pub parent_hash: Bytes32,        // Parent block hash

    // ── Chain position ──
    pub in_canonical_chain: bool,    // True if this block is in the canonical chain
    pub status: BlockStatus,         // Validation/finality status

    // ── Summary statistics ──
    pub timestamp: u64,              // Unix timestamp
    pub proposer_index: u32,         // Proposer validator index
    pub spend_bundle_count: u32,     // Transaction count
    pub total_cost: u64,             // CLVM execution cost
    pub total_fees: u64,             // Fees collected
    pub additions_count: u32,        // Coins created
    pub removals_count: u32,         // Coins spent
    pub block_size: u32,             // Serialized size (uncompressed)

    // ── L1 anchor ──
    pub l1_height: u32,              // L1 block height reference
    pub l1_hash: Bytes32,            // L1 block hash reference

    // ── State ──
    pub state_root: Bytes32,         // CoinSet state root after this block
}
```

**Construction:**

```rust
impl BlockRecord {
    /// Extracts a BlockRecord from an L2BlockHeader.
    /// Called automatically by BlockStore::put().
    pub fn from_header(header: &L2BlockHeader, status: BlockStatus) -> Self
}
```

### 3.3 StoredCheckpoint

A finalized checkpoint with its attestation metadata, stored in `CF_CHECKPOINTS`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCheckpoint {
    pub checkpoint: Checkpoint,            // The checkpoint data (from dig-block)
    pub signer_bitmap: SignerBitmap,       // Which validators signed
    pub aggregate_signature: Signature,    // BLS aggregate signature
    pub aggregate_pubkey: PublicKey,        // BLS aggregate public key
    pub score: u64,                        // Competition score
    pub submitter: u32,                    // Submitter validator index
    pub l1_height: Option<u32>,            // L1 confirmation height (if finalized on L1)
    pub l1_coin_id: Option<Bytes32>,       // L1 finalization coin ID
    pub stored_at: u64,                    // Unix timestamp when stored
}
```

### 3.4 ChainTip

The current chain tip, stored in `CF_METADATA` under key `META_TIP`.

```rust
#[derive(Debug, Clone, Copy)]
pub struct ChainTip {
    pub hash: Bytes32,
    pub height: u64,
}
```

**Encoding:** 40 bytes = `hash (32 bytes) || height (8 bytes little-endian u64)`.

### 3.5 StorageStats

Aggregate storage statistics for monitoring and diagnostics.

```rust
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub block_count: u64,              // Total blocks stored (all forks)
    pub canonical_block_count: u64,    // Blocks in canonical chain
    pub header_count: u64,             // Headers stored
    pub checkpoint_count: u64,         // Checkpoints stored
    pub attested_count: u64,           // Attested blocks stored
    pub tip_height: Option<u64>,       // Current chain tip height
    pub min_height: Option<u64>,       // Lowest stored height (after pruning)
    pub total_size_bytes: u64,         // Estimated total disk usage
}
```

### 3.6 BlockStoreConfig

```rust
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    /// Path to the RocksDB directory.
    pub path: std::path::PathBuf,

    // ── Caching ──

    /// In-memory LRU cache capacity for full blocks.
    pub block_cache_capacity: usize,    // Default: 1000

    /// In-memory LRU cache capacity for headers.
    pub header_cache_capacity: usize,   // Default: 2000

    /// Number of cache shards (must be power of 2).
    /// Higher values reduce lock contention under concurrent reads.
    pub cache_shards: usize,            // Default: 16

    /// Warm the block cache on startup by preloading recent blocks.
    pub warm_cache_on_open: bool,       // Default: true

    // ── RocksDB tuning ──

    /// RocksDB write buffer size.
    pub write_buffer_size: usize,       // Default: 64 MB

    /// RocksDB shared block cache size.
    pub block_cache_size: usize,        // Default: 128 MB

    /// Maximum open file descriptors.
    pub max_open_files: i32,            // Default: 1000

    /// Enable BlobDB for CF_BLOCKS (large value separation).
    pub enable_blob_db: bool,           // Default: true

    // ── Compression ──

    /// Enable zstd compression for full blocks.
    pub compress_blocks: bool,          // Default: true

    /// Zstd compression level (1-22).
    pub compression_level: i32,         // Default: 3

    /// Enable zstd dictionary compression.
    /// If true, a pre-trained dictionary is loaded from CF_METADATA
    /// or trained on the first 1000 blocks and stored.
    pub use_compression_dict: bool,     // Default: true

    // ── Write pipeline ──

    /// Maximum blocks to batch in a single WriteBatch during sync.
    pub write_pipeline_batch_size: usize,  // Default: 64

    /// Maximum delay (ms) before flushing a partial write batch.
    pub write_pipeline_flush_ms: u64,      // Default: 100

    // ── Durability ──

    /// Enable fsync on writes (durability vs performance).
    pub sync_writes: bool,              // Default: false

    // ── Pruning ──

    /// Enable compaction filter for automatic background pruning.
    /// When enabled, entries below min_height are dropped during
    /// compaction without an explicit prune_before_height() call.
    pub enable_compaction_pruning: bool, // Default: false

    /// Minimum height to keep (updated by prune_before_height()).
    /// Used by the compaction filter when enable_compaction_pruning is true.
    pub min_retained_height: Option<u64>, // Default: None (keep all)
}
```

## 4. Key Encoding

All RocksDB keys use fixed-size binary encodings for predictable sort order and efficient lookups.

### 4.1 Hash Keys (32 bytes)

Used for `CF_BLOCKS`, `CF_HEADERS`, `CF_ATTESTED`.

```
Key: block_hash.as_ref()  →  [u8; 32]  (raw bytes, no prefix)
```

Block hashes are uniformly distributed, providing natural load balancing across RocksDB's SST files.

### 4.2 Height Keys (8 bytes, big-endian)

Used for `CF_CANONICAL`.

```
Key: height.to_be_bytes()  →  [u8; 8]  (big-endian u64)
Value: block_hash.as_ref() →  [u8; 32]
```

Big-endian encoding ensures lexicographic order matches numeric order:
- Height 0: `0x0000000000000000`
- Height 1: `0x0000000000000001`
- Height 256: `0x0000000000000100`
- Height 2^64-1: `0xFFFFFFFFFFFFFFFF`

This enables efficient `prefix_iterator` and `range` queries on height ranges.

### 4.3 Epoch Keys (8 bytes, big-endian)

Used for `CF_CHECKPOINTS`.

```
Key: epoch.to_be_bytes()  →  [u8; 8]  (big-endian u64)
Value: bincode(StoredCheckpoint)
```

### 4.4 Metadata Keys (variable, UTF-8)

Used for `CF_METADATA`.

```
Key: key_name.as_bytes()  →  &[u8]  (UTF-8 string)
Value: arbitrary bytes
```

### 4.5 Key Encoding Summary

| Column Family | Key Type | Key Size | Key Encoding | Value | Value Encoding |
|---------------|----------|----------|-------------|-------|---------------|
| `CF_BLOCKS` | Block hash | 32 bytes | Raw `Bytes32` | `L2Block` | zstd-dict(bincode) via BlobDB |
| `CF_HEADERS` | Block hash | 32 bytes | Raw `Bytes32` | `L2BlockHeader` | bincode |
| `CF_ATTESTED` | Block hash | 32 bytes | Raw `Bytes32` | Attestation data | bincode |
| `CF_CANONICAL` | Height | 8 bytes | Big-endian u64 | Block hash | Raw `Bytes32` (durable backup; hot path uses mmap) |
| `CF_CHECKPOINTS` | Epoch | 8 bytes | Big-endian u64 | `StoredCheckpoint` | bincode |
| `CF_METADATA` | Name | Variable | UTF-8 bytes | Payload | Variable |
| `canonical.bin` | Height × 32 | — | Dense mmap | Block hash | Raw `Bytes32` at offset `height × 32` |

## 5. Block Storage & Retrieval

### 5.1 Storing a Block

```rust
/// Stores a validated block and updates all indexes.
///
/// Steps:
///   1. Compute block hash via L2BlockHeader::hash() (chia-sha2::Sha256)
///   2. Check if hash already exists in CF_HEADERS → return Ok(false) if so (idempotent)
///   3. Begin RocksDB WriteBatch:
///      a. CF_BLOCKS: hash → zstd_dict_compress(bincode(block)) (via BlobDB)
///      b. CF_HEADERS: hash → bincode(header)
///   4. If is_canonical:
///      c. CF_CANONICAL: height → hash
///      d. Append to canonical.bin mmap: hash at offset height × 32
///      e. CF_METADATA: "tip" → (hash, height) if height > current tip
///   5. Commit WriteBatch atomically
///   6. Update in-memory caches (block cache, header cache, BlockRecord cache)
///   7. Return Ok(true) — block was new
///
/// Idempotent: inserting an existing block returns Ok(false).
pub fn put(
    &self,
    block: &L2Block,
    is_canonical: bool,
) -> Result<bool, BlockStoreError>
```

### 5.1.1 Write Pipeline (Batch Mode)

During initial sync, single-block `put()` calls are bottlenecked by RocksDB WAL fsync. The write pipeline accepts blocks through an async channel and batches them:

```rust
/// Sends a block to the write pipeline for batched insertion.
/// Returns immediately. The block will be persisted in the next batch
/// (within write_pipeline_flush_ms or when batch_size is reached).
///
/// The returned oneshot receiver resolves when the block is durably stored.
pub async fn put_pipelined(
    &self,
    block: L2Block,
    is_canonical: bool,
) -> Result<oneshot::Receiver<Result<bool, BlockStoreError>>, BlockStoreError>
```

**Pipeline internals:**

```
                                          ┌─────────────────────────┐
put_pipelined(block_1) ──►               │                         │
put_pipelined(block_2) ──► mpsc channel ─►  Background batch task  │
put_pipelined(block_3) ──►               │  (collects up to 64     │
         ...           ──►               │   blocks or 100ms,      │
                                          │   issues one WriteBatch)│
                                          └─────────────────────────┘
```

The pipeline is transparent — `put()` still works for single-block insertion. `put_pipelined()` is used by the sync engine for throughput.

### 5.2 Retrieving a Block

```rust
/// Retrieves a full block by hash.
/// Checks in-memory cache first, then CF_BLOCKS.
/// Decompresses zstd on read.
pub fn get_block(&self, hash: &Bytes32) -> Result<Option<L2Block>, BlockStoreError>

/// Retrieves only the header by hash.
/// Checks header cache first, then CF_HEADERS.
pub fn get_header(&self, hash: &Bytes32) -> Result<Option<L2BlockHeader>, BlockStoreError>

/// Retrieves the block record (metadata) by hash.
/// Checks in-memory record cache first. On cache miss, deserializes
/// the header from CF_HEADERS and derives the record (sub-microsecond).
pub fn get_record(&self, hash: &Bytes32) -> Result<Option<BlockRecord>, BlockStoreError>

/// Retrieves the canonical block at a specific height.
/// Looks up hash in canonical.bin mmap (O(1)), then fetches from CF_BLOCKS.
pub fn get_block_by_height(&self, height: u64) -> Result<Option<L2Block>, BlockStoreError>

/// Retrieves only the canonical header at a specific height.
pub fn get_header_by_height(&self, height: u64) -> Result<Option<L2BlockHeader>, BlockStoreError>

/// Retrieves only the canonical block record at a specific height.
/// Derives from header on cache miss.
pub fn get_record_by_height(&self, height: u64) -> Result<Option<BlockRecord>, BlockStoreError>

/// Retrieves the canonical block hash at a specific height.
/// O(1) from canonical.bin mmap — a pointer dereference, not a DB read.
pub fn get_hash_by_height(&self, height: u64) -> Result<Option<Bytes32>, BlockStoreError>
```

### 5.3 Batch Retrieval

```rust
/// Retrieves multiple blocks by hash.
/// Uses RocksDB multi_get for batch efficiency.
pub fn get_blocks_by_hash(
    &self,
    hashes: &[Bytes32],
) -> Result<Vec<Option<L2Block>>, BlockStoreError>

/// Retrieves canonical blocks in a height range [start, end] inclusive.
/// Returns blocks in height order.
pub fn get_blocks_in_range(
    &self,
    start_height: u64,
    end_height: u64,
) -> Result<Vec<L2Block>, BlockStoreError>

/// Retrieves canonical block records in a height range [start, end] inclusive.
/// Lighter than get_blocks_in_range — no full block deserialization.
pub fn get_records_in_range(
    &self,
    start_height: u64,
    end_height: u64,
) -> Result<Vec<BlockRecord>, BlockStoreError>

/// Retrieves all canonical block hashes for a given epoch.
/// Uses epoch height arithmetic from dig-epoch to determine the range.
pub fn get_epoch_block_hashes(
    &self,
    epoch: u64,
) -> Result<Vec<Bytes32>, BlockStoreError>
```

### 5.4 Prefetching for Sequential Access

During sync serving (`get_blocks_in_range`), the access pattern is perfectly sequential. Prefetching optimizations:

```rust
/// Returns a streaming iterator of canonical blocks in a height range.
/// Uses readahead hints for sequential RocksDB access and pre-decompresses
/// the next N blocks while the caller processes the current one.
///
/// More efficient than get_blocks_in_range() for large ranges because
/// it doesn't buffer all blocks in memory at once.
pub fn stream_blocks_in_range(
    &self,
    start_height: u64,
    end_height: u64,
) -> impl Iterator<Item = Result<L2Block, BlockStoreError>> + '_
```

**Prefetch internals:**
- `ReadOptions::set_readahead_size(2 * 1024 * 1024)` for RocksDB iterators (2MB readahead)
- RocksDB `MultiGet` for batch hash lookups from `CF_CANONICAL` → `CF_BLOCKS` (vectored I/O)
- Background decompression of the next block while the current one is being processed

### 5.5 Async API

All read methods have async counterparts that serve cache hits on the tokio executor and dispatch cache-miss reads to `spawn_blocking`:

```rust
/// Async version of get_block(). Cache hit stays on the async executor
/// (no thread switch). Cache miss dispatches to the blocking threadpool.
pub async fn get_block_async(
    &self,
    hash: &Bytes32,
) -> Result<Option<L2Block>, BlockStoreError>

/// Async version of get_header().
pub async fn get_header_async(
    &self,
    hash: &Bytes32,
) -> Result<Option<L2BlockHeader>, BlockStoreError>

/// Async version of get_block_by_height(). The mmap canonical lookup
/// and cache check stay on the async executor.
pub async fn get_block_by_height_async(
    &self,
    height: u64,
) -> Result<Option<L2Block>, BlockStoreError>
```

The pattern applies to all read methods. Write methods (`put`, `extend_chain`, `rollback_to_height`, `apply_reorg`) also have async versions that dispatch the `WriteBatch` to the blocking pool.

### 5.6 Attestation Storage

```rust
/// Stores attestation data for an existing block.
/// The block must already exist in CF_BLOCKS.
/// Updates the BlockRecord status.
pub fn put_attestation(
    &self,
    hash: &Bytes32,
    attested: &AttestedBlock,
) -> Result<(), BlockStoreError>

/// Retrieves attestation data for a block.
pub fn get_attestation(
    &self,
    hash: &Bytes32,
) -> Result<Option<AttestedBlock>, BlockStoreError>
```

## 6. Canonical Chain Management

### 6.1 Canonical Index (Dual-Layer)

The canonical chain uses a **dual-layer** architecture for the `height → hash` mapping:

**Hot layer: Memory-mapped file (`canonical.bin`)**
- Dense array of 32-byte hashes: `hash_at_height_h` is at offset `h × 32`
- Lookup is a pointer dereference into the OS page cache — zero syscalls, zero serialization
- Append on new canonical block: write 32 bytes at offset `height × 32`
- Rollback: truncate file to `(target_height + 1) × 32` bytes
- Rebuilt from `CF_CANONICAL` on startup if the file is missing or corrupt

**Cold layer: RocksDB `CF_CANONICAL`**
- Durable backup written in the same `WriteBatch` as the block
- Used only for crash recovery and snapshot export
- Big-endian u64 keys for natural sort order

```
canonical.bin (mmap):
  Offset 0:   [hash_at_height_0  (32 bytes)]
  Offset 32:  [hash_at_height_1  (32 bytes)]
  Offset 64:  [hash_at_height_2  (32 bytes)]
  ...
  Offset N×32: [hash_at_height_N (32 bytes)]  ← tip

CF_CANONICAL (durable backup):
  0x0000000000000000 → hash_at_height_0
  0x0000000000000001 → hash_at_height_1
  ...
```

**Why dual-layer:** `get_hash_by_height()` is the single hottest path in the store — called on every block during sync, validation, and serving. The mmap lookup is ~10ns (cache-line read). A RocksDB point-lookup is ~1-10μs (memtable check + bloom filter + possible disk read). The 100-1000x speedup justifies the complexity of maintaining two representations.

### 6.2 Setting Canonical Chain

```rust
/// Marks a block as canonical at its height.
/// Updates CF_CANONICAL and the block's BlockRecord.in_canonical_chain flag.
/// If this height already has a different canonical block, the old block's
/// record is updated to in_canonical_chain=false.
pub fn set_canonical(&self, hash: &Bytes32) -> Result<(), BlockStoreError>

/// Marks a sequence of blocks as canonical (used during reorg).
/// More efficient than calling set_canonical() in a loop — uses a single WriteBatch.
pub fn set_canonical_batch(
    &self,
    hashes: &[Bytes32],
) -> Result<(), BlockStoreError>
```

### 6.3 Extending the Canonical Chain

```rust
/// Stores a block and extends the canonical chain in one atomic operation.
/// Equivalent to put(block, is_canonical=true) but makes intent explicit.
/// Updates the tip if the block's height exceeds the current tip.
pub fn extend_chain(
    &self,
    block: &L2Block,
) -> Result<bool, BlockStoreError>
```

## 7. Chain Tip & Peak Tracking

### 7.1 Reading the Tip

```rust
/// Returns the current chain tip (hash, height).
/// O(1) from in-memory cache, backed by CF_METADATA.
pub fn tip(&self) -> Option<ChainTip>

/// Returns the current chain height (tip height).
/// Convenience: tip().map(|t| t.height).
pub fn height(&self) -> Option<u64>
```

### 7.2 Updating the Tip

The tip is updated automatically by `put()` (when `is_canonical=true` and height > current tip) and by `rollback()`. It can also be set explicitly:

```rust
/// Sets the chain tip to a specific block.
/// The block must exist and be marked canonical.
pub fn set_tip(&self, hash: &Bytes32) -> Result<(), BlockStoreError>
```

### 7.3 Tip Encoding

Stored in `CF_METADATA` under key `"tip"`:

```
Bytes: [hash (32 bytes)] [height (8 bytes LE u64)]
Total: 40 bytes
```

## 8. Rollback & Reorg Support

### 8.1 Rollback to Height

```rust
/// Rolls back the canonical chain to the specified height.
///
/// Steps:
///   1. Validate: target_height < current tip height
///   2. Collect all canonical hashes at heights (target_height + 1) .. tip
///   3. Begin WriteBatch:
///      a. Delete CF_CANONICAL entries for heights > target_height
///      b. Update CF_RECORDS for each rolled-back block: in_canonical_chain = false
///      c. Update CF_METADATA tip to block at target_height
///   4. Commit atomically
///   5. Update in-memory caches
///   6. Return the list of rolled-back block hashes
///
/// The rolled-back blocks remain in CF_BLOCKS and CF_HEADERS —
/// they are not deleted, only deindexed from the canonical chain.
pub fn rollback_to_height(
    &self,
    target_height: u64,
) -> Result<Vec<Bytes32>, BlockStoreError>
```

### 8.2 Reorg Support

The block store does not decide which fork wins — that's the consensus layer's job. But it provides the primitives the consensus layer needs:

```rust
/// Finds the common ancestor between the current canonical chain
/// and a new block by walking back parent hashes.
///
/// Returns (ancestor_hash, ancestor_height) or None if no common ancestor
/// is found within max_depth.
pub fn find_common_ancestor(
    &self,
    block_hash: &Bytes32,
    max_depth: u64,
) -> Result<Option<(Bytes32, u64)>, BlockStoreError>

/// Returns the canonical block hashes that would be reverted
/// if the chain rolled back to the given height.
/// Returns hashes in descending height order (tip first).
pub fn blocks_to_revert(
    &self,
    target_height: u64,
) -> Result<Vec<Bytes32>, BlockStoreError>

/// Executes a full reorg: rollback to common ancestor, then
/// set new blocks as canonical.
///
/// Steps:
///   1. rollback_to_height(ancestor_height)
///   2. set_canonical_batch(new_chain_hashes) in height order
///   3. Update tip to the last block in new_chain_hashes
///
/// All steps execute in a single WriteBatch for atomicity.
pub fn apply_reorg(
    &self,
    ancestor_height: u64,
    new_chain_hashes: &[Bytes32],
) -> Result<ReorgResult, BlockStoreError>
```

```rust
#[derive(Debug, Clone)]
pub struct ReorgResult {
    pub reverted: Vec<Bytes32>,      // Blocks removed from canonical chain
    pub applied: Vec<Bytes32>,       // Blocks added to canonical chain
    pub new_tip: ChainTip,           // New chain tip after reorg
}
```

## 9. Checkpoint Storage

### 9.1 Storing Checkpoints

```rust
/// Stores a finalized checkpoint for an epoch.
/// Keyed by epoch number in CF_CHECKPOINTS.
/// Idempotent: re-storing the same epoch replaces the previous entry.
pub fn put_checkpoint(
    &self,
    stored: &StoredCheckpoint,
) -> Result<(), BlockStoreError>
```

### 9.2 Retrieving Checkpoints

```rust
/// Retrieves the stored checkpoint for an epoch.
pub fn get_checkpoint(
    &self,
    epoch: u64,
) -> Result<Option<StoredCheckpoint>, BlockStoreError>

/// Retrieves the most recent stored checkpoint.
/// Scans CF_CHECKPOINTS in reverse key order.
pub fn get_latest_checkpoint(
    &self,
) -> Result<Option<StoredCheckpoint>, BlockStoreError>

/// Retrieves checkpoints in an epoch range [start, end] inclusive.
pub fn get_checkpoints_in_range(
    &self,
    start_epoch: u64,
    end_epoch: u64,
) -> Result<Vec<StoredCheckpoint>, BlockStoreError>
```

## 10. Pruning & Archival

### 10.1 Pruning Blocks

```rust
/// Removes all blocks, headers, records, and canonical entries
/// at heights strictly below min_height.
///
/// Attestation data for pruned blocks is also removed.
/// Checkpoints are NOT pruned (they are epoch-level, not height-level).
///
/// Updates CF_METADATA "min_height" to min_height.
/// Returns the number of blocks removed.
pub fn prune_before_height(
    &self,
    min_height: u64,
) -> Result<usize, BlockStoreError>
```

### 10.2 Pruning Checkpoints

```rust
/// Removes checkpoints for epochs strictly below min_epoch.
/// Returns the number of checkpoints removed.
pub fn prune_checkpoints_before_epoch(
    &self,
    min_epoch: u64,
) -> Result<usize, BlockStoreError>
```

### 10.3 Compaction Filter (Background Pruning)

When `enable_compaction_pruning` is set in `BlockStoreConfig`, a RocksDB compaction filter automatically drops entries below `min_retained_height` during background compaction:

```rust
struct PruneCompactionFilter {
    min_height: Arc<AtomicU64>,
}

impl CompactionFilter for PruneCompactionFilter {
    fn filter(&self, _level: u32, key: &[u8], _value: &[u8]) -> Decision {
        // Only applies to CF_BLOCKS, CF_HEADERS, CF_ATTESTED
        // (hash-keyed CFs require cross-referencing BlockRecord height;
        //  in practice, prune_before_height() handles these explicitly,
        //  and the compaction filter handles CF_CANONICAL)
        Decision::Keep
    }
}
```

For `CF_CANONICAL` (height-keyed), the filter can trivially compare the key against `min_height`. For hash-keyed CFs, explicit `prune_before_height()` is still needed because the key alone doesn't reveal the block's height.

The compaction filter amortizes `CF_CANONICAL` pruning across normal compaction cycles, avoiding a separate pruning pass.

### 10.4 Pruning Strategy

Pruning is always initiated externally — the block store never decides what to prune. The consensus layer decides when and what to prune based on:
- Finalization depth (prune blocks older than the last hard-finalized epoch)
- Disk space constraints
- Sync requirements (keep enough for peers to sync from)

The compaction filter (§10.3) provides a low-overhead assist for height-keyed data, but hash-keyed data requires explicit `prune_before_height()` calls.

## 11. Caching Strategy

### 11.1 Sharded Block Cache

```rust
pub struct ShardedCache<V> {
    shards: Vec<RwLock<LruCache<Bytes32, V>>>,
    shard_count: usize, // must be power of 2
}

impl<V> ShardedCache<V> {
    fn shard_for(&self, key: &Bytes32) -> usize {
        // First byte of hash is already well-distributed (SHA-256 output)
        key.as_ref()[0] as usize & (self.shard_count - 1)
    }
}
```

- **Type:** `ShardedCache<L2Block>` — 16 shards, each an `LruCache`
- **Total capacity:** Configurable (default 1000 blocks spread across shards)
- **Eviction:** LRU per shard
- **Population:** Blocks are cached on `put()` and on cache-miss `get_block()`
- **Invalidation:** Entries are removed on `prune_before_height()`
- **Concurrency:** 16 shards reduce lock contention by ~16x under concurrent RPC load. A single `RwLock<LruCache>` becomes a bottleneck at 10+ reader threads; sharding eliminates this.

### 11.2 Sharded Header Cache

- **Type:** `ShardedCache<L2BlockHeader>` — 16 shards
- **Total capacity:** Configurable (default 2000 headers)
- **Eviction:** LRU per shard
- **Population:** Headers are cached on `put()` and on cache-miss `get_header()`

### 11.3 BlockRecord Cache

- **Type:** `ShardedCache<BlockRecord>` — 16 shards
- **Total capacity:** Same as header cache (records are derived from headers)
- **Population:** Records are cached on `put()` (derived from header) and on cache-miss `get_record()` (derived from `CF_HEADERS` header deserialization)
- **Not persisted:** Records are in-memory only, re-derived from headers on cache miss

### 11.4 Canonical Height Index

- **Type:** Memory-mapped file `canonical.bin` (see §6.1)
- **Scope:** Full canonical chain from genesis to tip
- **Lookup:** O(1) — pointer dereference at `offset = height × 32`
- **Update:** Append 32 bytes on `extend_chain()`, truncate on `rollback()`
- **Persistence:** Durable via mmap + `CF_CANONICAL` backup

### 11.5 Hash-to-Height Cache

- **Type:** `HashMap<Bytes32, u64>` (hash → height)
- **Scope:** Recent heights (tip - N .. tip)
- **Purpose:** Reverse lookup to find a block's height from its hash without scanning the mmap file

### 11.6 Cache Warming on Startup

When `warm_cache_on_open` is enabled (default), the block store preloads the most recent blocks into the caches during `open()`:

```rust
fn warm_caches(&self) -> Result<(), BlockStoreError> {
    let tip = match self.tip() {
        Some(t) => t,
        None => return Ok(()), // empty store
    };
    let start = tip.height.saturating_sub(self.config.block_cache_capacity as u64);
    for height in start..=tip.height {
        // get_block_by_height() populates block cache, header cache,
        // and record cache on read
        let _ = self.get_block_by_height(height)?;
    }
    Ok(())
}
```

This eliminates the cold-start penalty where the first N requests after restart would all be cache misses. Warming reads are sequential and benefit from RocksDB readahead.

## 12. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum BlockStoreError {
    /// RocksDB I/O error.
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    /// Serialization/deserialization failure.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Zstd compression/decompression failure.
    #[error("Compression error: {0}")]
    Compression(String),

    /// Block not found by hash.
    #[error("Block not found: {0}")]
    BlockNotFound(Bytes32),

    /// Checkpoint not found for epoch.
    #[error("Checkpoint not found for epoch {0}")]
    CheckpointNotFound(u64),

    /// Attempted to set canonical for a block that doesn't exist.
    #[error("Cannot set canonical: block {0} not in store")]
    BlockNotInStore(Bytes32),

    /// Attempted to rollback below the minimum stored height.
    #[error("Cannot rollback to {target}: minimum stored height is {min}")]
    RollbackBelowMin { target: u64, min: u64 },

    /// Attempted to rollback above the current tip.
    #[error("Cannot rollback to {target}: current tip is {tip}")]
    RollbackAboveTip { target: u64, tip: u64 },

    /// No chain tip is set (empty store).
    #[error("No chain tip set")]
    NoTip,

    /// Schema version mismatch.
    #[error("Schema version mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    /// Store is not initialized (missing genesis).
    #[error("Store not initialized: call init_genesis() first")]
    NotInitialized,
}
```

## 13. Serialization

### 13.1 Block Serialization (Dictionary Compression)

Full blocks are serialized with bincode, then compressed with a pre-trained zstd dictionary:

```
Write path:  L2Block → bincode::serialize()
                     → zstd::bulk::compress_with_dictionary(dict, level=3)
                     → CF_BLOCKS (via BlobDB)

Read path:   CF_BLOCKS (via BlobDB)
           → zstd::bulk::decompress_with_dictionary(dict)
           → bincode::deserialize()
           → L2Block
```

**Dictionary training:** On first startup with no dictionary, the store operates in plain-zstd mode. After 1000 blocks are stored, a dictionary is trained via `zstd::dict::from_samples()` on a random sample of block bodies and persisted to `CF_METADATA` under key `META_ZSTD_DICT`. Subsequent startups load the dictionary from metadata. The dictionary is ~100KB and provides 20-40% better compression than plain zstd because L2 block bodies have highly repetitive structure (same field layout, similar SpendBundle shapes).

**Fallback:** If dictionary decompression fails (e.g., block was written before dictionary was trained), the store falls back to plain zstd decompression. This handles the transition transparently.

### 13.2 Header Serialization

Headers are serialized with bincode only (no compression — small and frequently read):

```
Write path:  L2BlockHeader → bincode::serialize() → CF_HEADERS
Read path:   CF_HEADERS → bincode::deserialize() → L2BlockHeader
```

BlockRecords are not persisted — they are derived from headers on the fly and cached in memory (see §11.3).

### 13.3 Wire-Format Interop

For serving blocks to peers via `dig-gossip`, blocks can be exported using `chia-traits::Streamable`:

```rust
/// Serializes a block to Chia Streamable wire format for peer gossip.
/// Uses chia-traits::Streamable, not bincode.
pub fn block_to_wire_bytes(block: &L2Block) -> Vec<u8>

/// Deserializes a block from Chia Streamable wire format.
pub fn block_from_wire_bytes(bytes: &[u8]) -> Result<L2Block, BlockStoreError>
```

### 13.4 Round-Trip Guarantees

- `bincode::deserialize(bincode::serialize(x)) == x` for all stored types.
- `zstd::decode_all(zstd::encode_all(x)) == x` for all compression levels.
- Dictionary-compressed blocks decompress identically to the original.
- `block.hash()` is invariant across serialize/deserialize cycles.

## 14. Snapshot Export & Import

Snapshot support enables new nodes to bootstrap from a checkpoint instead of replaying from genesis. Combined with `dig-coinstore` state snapshots and `dig-epoch` checkpoints, this enables sync in minutes instead of hours.

### 14.1 Export

```rust
/// Exports canonical blocks in [start_height, end_height] as a streaming snapshot.
///
/// Format:
///   [manifest: SnapshotManifest (bincode)]
///   [block_0_len: u32 LE] [block_0_compressed_bytes]
///   [block_1_len: u32 LE] [block_1_compressed_bytes]
///   ...
///   [checksum: Bytes32 (SHA-256 of all preceding bytes)]
///
/// Blocks are written pre-compressed (same zstd-dict format as CF_BLOCKS)
/// to avoid decompressing and recompressing during export.
pub fn export_snapshot(
    &self,
    start_height: u64,
    end_height: u64,
    writer: &mut impl std::io::Write,
) -> Result<SnapshotManifest, BlockStoreError>
```

### 14.2 Import

```rust
/// Imports a snapshot, adding all blocks as canonical.
///
/// Validates:
///   - Manifest schema version matches
///   - Trailing checksum matches SHA-256 of snapshot body
///   - Block heights are contiguous
///   - Each block's parent_hash matches the previous block's hash
///
/// Uses the write pipeline internally for batched insertion.
pub fn import_snapshot(
    &self,
    reader: &mut impl std::io::Read,
) -> Result<SnapshotManifest, BlockStoreError>
```

### 14.3 SnapshotManifest

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub start_height: u64,
    pub end_height: u64,
    pub block_count: u64,
    pub start_hash: Bytes32,          // Hash of first block in snapshot
    pub end_hash: Bytes32,            // Hash of last block (new tip after import)
    pub total_bytes: u64,             // Total snapshot size
    pub compressed: bool,             // Whether blocks are pre-compressed
    pub checksum: Bytes32,            // SHA-256 of snapshot body (chia-sha2)
}
```

## 15. Public API Summary

### 15.1 Construction

| Function | Signature | Description |
|----------|-----------|-------------|
| `BlockStore::open()` | `(BlockStoreConfig) -> Result<Self>` | Opens or creates a block store at the configured path. |
| `BlockStore::open_readonly()` | `(path) -> Result<Self>` | Opens an existing store in read-only mode. |
| `BlockStore::init_genesis()` | `(&self, genesis: &L2Block) -> Result<()>` | Initializes the store with the genesis block. Must be called once on a fresh store. |

### 15.2 Block Storage

| Function | Signature | Description |
|----------|-----------|-------------|
| `put()` | `(&self, &L2Block, bool) -> Result<bool>` | Store block, optionally as canonical. Returns false if duplicate. |
| `put_pipelined()` | `async (&self, L2Block, bool) -> Result<Receiver<Result<bool>>>` | Batched write via async pipeline (sync throughput). |
| `extend_chain()` | `(&self, &L2Block) -> Result<bool>` | Store block and extend canonical chain. |
| `put_attestation()` | `(&self, &Bytes32, &AttestedBlock) -> Result<()>` | Store attestation for existing block. |

### 15.3 Block Retrieval

| Function | Signature | Description | Chia crates used |
|----------|-----------|-------------|-----------------|
| `get_block()` | `(&self, &Bytes32) -> Result<Option<L2Block>>` | Full block by hash (cache → RocksDB). | — |
| `get_header()` | `(&self, &Bytes32) -> Result<Option<L2BlockHeader>>` | Header by hash. | — |
| `get_record()` | `(&self, &Bytes32) -> Result<Option<BlockRecord>>` | Lightweight record by hash. | — |
| `get_attestation()` | `(&self, &Bytes32) -> Result<Option<AttestedBlock>>` | Attestation by hash. | — |
| `get_block_by_height()` | `(&self, u64) -> Result<Option<L2Block>>` | Canonical block at height. | — |
| `get_header_by_height()` | `(&self, u64) -> Result<Option<L2BlockHeader>>` | Canonical header at height. | — |
| `get_record_by_height()` | `(&self, u64) -> Result<Option<BlockRecord>>` | Canonical record at height. | — |
| `get_hash_by_height()` | `(&self, u64) -> Result<Option<Bytes32>>` | Canonical hash at height. | — |
| `get_blocks_by_hash()` | `(&self, &[Bytes32]) -> Result<Vec<Option<L2Block>>>` | Batch retrieval by hash. | — |
| `get_blocks_in_range()` | `(&self, u64, u64) -> Result<Vec<L2Block>>` | Canonical blocks in height range. | — |
| `get_records_in_range()` | `(&self, u64, u64) -> Result<Vec<BlockRecord>>` | Canonical records in height range. | — |
| `get_epoch_block_hashes()` | `(&self, u64) -> Result<Vec<Bytes32>>` | Canonical hashes in epoch. | `dig-epoch` (height arithmetic) |
| `block_to_wire_bytes()` | `(&L2Block) -> Vec<u8>` | Export to Chia Streamable format. | `chia-traits::Streamable` |
| `block_from_wire_bytes()` | `(&[u8]) -> Result<L2Block>` | Import from Chia Streamable format. | `chia-traits::Streamable` |
| `stream_blocks_in_range()` | `(&self, u64, u64) -> impl Iterator<Item=Result<L2Block>>` | Streaming range with prefetch. | — |
| `get_block_async()` | `async (&self, &Bytes32) -> Result<Option<L2Block>>` | Async block retrieval. | — |
| `get_header_async()` | `async (&self, &Bytes32) -> Result<Option<L2BlockHeader>>` | Async header retrieval. | — |
| `get_block_by_height_async()` | `async (&self, u64) -> Result<Option<L2Block>>` | Async canonical block by height. | — |

### 15.4 Chain Management

| Function | Signature | Description |
|----------|-----------|-------------|
| `tip()` | `(&self) -> Option<ChainTip>` | Current chain tip. |
| `height()` | `(&self) -> Option<u64>` | Current chain height. |
| `set_tip()` | `(&self, &Bytes32) -> Result<()>` | Set tip to specific block. |
| `set_canonical()` | `(&self, &Bytes32) -> Result<()>` | Mark block as canonical. |
| `set_canonical_batch()` | `(&self, &[Bytes32]) -> Result<()>` | Mark multiple blocks canonical. |
| `find_common_ancestor()` | `(&self, &Bytes32, u64) -> Result<Option<(Bytes32, u64)>>` | Find fork point. |
| `blocks_to_revert()` | `(&self, u64) -> Result<Vec<Bytes32>>` | Blocks above target height. |
| `rollback_to_height()` | `(&self, u64) -> Result<Vec<Bytes32>>` | Rollback canonical chain. |
| `apply_reorg()` | `(&self, u64, &[Bytes32]) -> Result<ReorgResult>` | Atomic rollback + apply. |

### 15.5 Checkpoint Storage

| Function | Signature | Description |
|----------|-----------|-------------|
| `put_checkpoint()` | `(&self, &StoredCheckpoint) -> Result<()>` | Store checkpoint by epoch. |
| `get_checkpoint()` | `(&self, u64) -> Result<Option<StoredCheckpoint>>` | Get checkpoint for epoch. |
| `get_latest_checkpoint()` | `(&self) -> Result<Option<StoredCheckpoint>>` | Most recent checkpoint. |
| `get_checkpoints_in_range()` | `(&self, u64, u64) -> Result<Vec<StoredCheckpoint>>` | Checkpoints in epoch range. |

### 15.6 Snapshot

| Function | Signature | Description |
|----------|-----------|-------------|
| `export_snapshot()` | `(&self, u64, u64, &mut impl Write) -> Result<SnapshotManifest>` | Export canonical blocks in range. |
| `import_snapshot()` | `(&self, &mut impl Read) -> Result<SnapshotManifest>` | Import snapshot (batched via write pipeline). |

### 15.7 Maintenance

| Function | Signature | Description |
|----------|-----------|-------------|
| `prune_before_height()` | `(&self, u64) -> Result<usize>` | Remove blocks below height. |
| `prune_checkpoints_before_epoch()` | `(&self, u64) -> Result<usize>` | Remove old checkpoints. |
| `stats()` | `(&self) -> Result<StorageStats>` | Storage statistics. |
| `flush()` | `(&self) -> Result<()>` | Force RocksDB WAL flush. |
| `compact()` | `(&self) -> Result<()>` | Trigger RocksDB compaction. |

### 15.8 Status Updates

| Function | Signature | Description |
|----------|-----------|-------------|
| `update_status()` | `(&self, &Bytes32, BlockStatus) -> Result<()>` | Update a block's status in its BlockRecord (in-memory cache). |

## 16. Crate Boundary

### 16.1 What This Crate Owns

| Concern | Owned by `dig-blockstore` | Crates used |
|---------|--------------------------|-------------|
| Block persistence (L2Block, zstd-compressed) | Yes | `dig-block` (`L2Block`), `zstd`, `bincode` |
| Header persistence (L2BlockHeader) | Yes | `dig-block` (`L2BlockHeader`), `bincode` |
| Attestation persistence (AttestedBlock) | Yes | `dig-block` (`AttestedBlock`), `bincode` |
| Block record metadata (BlockRecord, in-memory only) | Yes | `dig-block` (`L2BlockHeader`, `BlockStatus`) |
| Canonical chain index (mmap + RocksDB dual-layer) | Yes | `chia-protocol` (`Bytes32`), `memmap2` |
| Chain tip tracking | Yes | `chia-protocol` (`Bytes32`) |
| Checkpoint persistence (StoredCheckpoint) | Yes | `dig-block` (`Checkpoint`, `CheckpointSubmission`, `SignerBitmap`), `chia-bls` (`Signature`, `PublicKey`) |
| Fork block storage (non-canonical, by hash) | Yes | — |
| Rollback and reorg primitives | Yes | — |
| Pruning (blocks, checkpoints) | Yes | — |
| Sharded in-memory caching (blocks, headers, records) | Yes | `lru`, `parking_lot` |
| Write pipeline (async batched ingestion) | Yes | `tokio` |
| Async API layer | Yes | `tokio` |
| Snapshot export/import | Yes | `chia-sha2` (checksum), `zstd` |
| RocksDB column family management (BlobDB, per-CF compaction, filters) | Yes | `rocksdb` |
| Dictionary-trained zstd compression | Yes | `zstd` |
| Wire-format serialization for peer serving | Yes | `chia-traits` (`Streamable`) |
| Storage statistics and diagnostics | Yes | — |
| Error types (`BlockStoreError`) | Yes | `thiserror` |

### 16.2 What This Crate Does NOT Own

| Concern | Owned by | Notes |
|---------|----------|-------|
| Block type definitions (`L2BlockHeader`, `L2Block`, `AttestedBlock`) | `dig-block` | `dig-blockstore` stores these, never redefines them |
| Block validation (structural, execution, state) | `dig-block` | Blocks are validated before storage |
| Block production (`BlockBuilder`) | `dig-block` | — |
| Checkpoint type definitions (`Checkpoint`, `CheckpointSubmission`) | `dig-block` | Stored, not defined |
| `BlockStatus` enum definition | `dig-block` | Used in `BlockRecord`, not defined |
| Epoch lifecycle (phase management, competition) | `dig-epoch` | `dig-blockstore` stores epoch artifacts |
| Epoch height arithmetic | `dig-epoch` | Used for `get_epoch_block_hashes()` |
| Global coin state (UTXO set, state roots) | `dig-coinstore` | — |
| Fork choice policy (which fork wins) | Consensus layer | `dig-blockstore` executes reorg when told |
| Transaction pool | `dig-mempool` | — |
| Networking (block gossip, sync) | `dig-gossip` | — |
| CLVM execution | `dig-clvm` | — |
| `Bytes32` type | `chia-protocol` crate (Chia) | Used directly for all hash keys |
| `Signature`, `PublicKey` types | `chia-bls` crate (Chia) | Stored inside blocks/checkpoints |
| SHA-256 hashing | `chia-sha2` crate (Chia) | Used for hash verification on read |
| Wire serialization (`Streamable` trait) | `chia-traits` crate (Chia) | Used for gossip export |
| Network constants | `dig-constants` | — |

### 16.3 Dependency Direction

```
dig-blockstore  (this crate — block persistence, chain indexing)
    │
    │  ┌─── DIG ecosystem ──────────────────────────────────────────────────┐
    ├──► dig-block         (L2BlockHeader, L2Block, AttestedBlock, Checkpoint,
    │                       CheckpointSubmission, BlockStatus, SignerBitmap,
    │                       ReceiptList — ALL block types, never redefined)
    ├──► dig-epoch         (epoch_for_block_height, epoch_checkpoint_height,
    │                       first_height_in_epoch — height arithmetic for
    │                       epoch-based queries)
    ├──► dig-constants     (NetworkConstants, network ID)
    │  └────────────────────────────────────────────────────────────────────┘
    │
    │  ┌─── Chia ecosystem (used directly for types, hashing, wire format) ┐
    ├──► chia-protocol     (Bytes32 — all hash keys)
    ├──► chia-bls          (Signature, PublicKey — stored in blocks/checkpoints)
    ├──► chia-sha2         (Sha256 — hash verification on read-back)
    ├──► chia-traits       (Streamable — wire-format export for gossip)
    │  └───────────────────────────────────────────────────────────────────┘
    │
    ├──► rocksdb           (persistent storage backend, BlobDB for CF_BLOCKS,
    │                       per-CF compaction strategies, compaction filters)
    ├──► zstd              (dictionary-trained block compression)
    ├──► bincode           (serialization for all stored types)
    ├──► serde             (derive Serialize/Deserialize)
    ├──► lru               (sharded LRU caches for blocks, headers, records)
    ├──► tokio             (async API, write pipeline channel, spawn_blocking)
    ├──► memmap2           (memory-mapped canonical height index)
    ├──► thiserror         (error derivation)
    └──► parking_lot       (RwLock for cache shard concurrency)

Downstream consumers:
    chain manager  ──► dig-blockstore  (extend_chain, rollback_to_height, apply_reorg,
                                        get_block_by_height, find_common_ancestor)
    consensus      ──► dig-blockstore  (put_checkpoint, get_checkpoint, update_status,
                                        blocks_to_revert)
    block proposer ──► dig-blockstore  (get_header_by_height — parent hash lookup,
                                        get_record_by_height — fee/cost reference)
    dig-coinstore  ──► dig-blockstore  (get_blocks_in_range — replay blocks for state)
    dig-gossip     ──► dig-blockstore  (get_block, block_to_wire_bytes — serve to peers)
    full-node RPC  ──► dig-blockstore  (get_block, get_header, get_record, tip,
                                        get_records_in_range — API responses)
    sync engine    ──► dig-blockstore  (put, get_hash_by_height — batch sync,
                                        get_blocks_in_range — serve sync requests)
    dig-block      ──  (no dependency — dig-block does NOT depend on dig-blockstore)
    dig-epoch      ──  (no dependency — dig-epoch does NOT depend on dig-blockstore)
```

**Note:** The dependency is strictly one-directional: `dig-blockstore` depends on `dig-block` and `dig-epoch`, never the reverse. `dig-block` defines block types; `dig-blockstore` persists them. `dig-epoch` defines epoch arithmetic; `dig-blockstore` uses it for epoch-scoped queries.

## 17. Testing Strategy

### 17.1 Unit Tests

| Category | Tests |
|----------|-------|
| **BlockRecord** | `from_header()` extracts all fields correctly. `in_canonical_chain` defaults to value passed. Serialization round-trip preserves all fields. |
| **StoredCheckpoint** | Construction with all fields. Serialization round-trip. Optional fields (`l1_height`, `l1_coin_id`) handle `None` correctly. |
| **ChainTip** | Encoding: 40 bytes = hash (32) + height LE (8). Decoding round-trip. |
| **Key encoding** | Height 0 → `0x0000000000000000`. Height 1 → `0x0000000000000001`. Height 2^32 → correct 8-byte BE. Epoch keys same pattern. Hash keys are raw 32 bytes. |
| **Key sort order** | Heights encode in ascending lexicographic order: `key(1) < key(2) < key(1000)`. |
| **BlockStoreConfig** | Defaults are correct. Custom values propagate to RocksDB options. |
| **BlockStoreError** | All variants produce meaningful Display messages. `From<rocksdb::Error>` conversion works. |

### 17.2 Block Storage Tests

| Category | Tests |
|----------|-------|
| **put() + get_block()** | Store block, retrieve by hash, verify identical. |
| **put() + get_header()** | Store block, retrieve header only, verify header matches block.header. |
| **put() + get_record()** | Store block, retrieve record, verify all metadata fields. |
| **Idempotent put()** | Store same block twice, second returns `Ok(false)`. Block is unchanged. |
| **get_block() cache hit** | Store block, get once (populates cache), get again (served from cache). Verify both return identical block. |
| **get_block() cache miss** | Store block, evict from cache manually, get (served from RocksDB). |
| **Compression** | Store block with compression enabled, verify compressed size < uncompressed. Retrieve and verify identical to original. |
| **Non-existent block** | `get_block(random_hash)` returns `Ok(None)`. |
| **Hash verification** | Store block, retrieve, verify `header.hash() == expected_hash` using `chia-sha2::Sha256`. |
| **Attestation storage** | Store block, then `put_attestation()`, then `get_attestation()`. Verify all fields. Attestation for non-existent block returns error. |

### 17.3 Canonical Chain Tests

| Category | Tests |
|----------|-------|
| **extend_chain()** | Store genesis, extend with block 2, 3, 4. Verify `tip()` updates. Verify `get_block_by_height()` returns correct block at each height. |
| **get_hash_by_height()** | After extending, each height returns correct hash. Height beyond tip returns `None`. |
| **get_blocks_in_range()** | Extend to height 10. Range [3, 7] returns 5 blocks in order. Range [0, 100] returns all blocks. Empty range returns empty vec. |
| **get_records_in_range()** | Same as above but returns `BlockRecord` (no decompression). |
| **Fork block not in canonical** | Store block A at height 5 (canonical). Store block B at height 5 (not canonical). `get_block_by_height(5)` returns A. `get_block(B.hash)` returns B. |
| **set_canonical() switches fork** | Store A and B at height 5. A is canonical. `set_canonical(B.hash)`. Now `get_block_by_height(5)` returns B. A's record has `in_canonical_chain=false`. |
| **get_epoch_block_hashes()** | Extend chain through 2 epochs (64 blocks). `get_epoch_block_hashes(0)` returns 32 hashes. `get_epoch_block_hashes(1)` returns 32 hashes. Verify they match `dig-epoch::first_height_in_epoch()` through `epoch_checkpoint_height()`. |

### 17.4 Rollback Tests

| Category | Tests |
|----------|-------|
| **rollback_to_height()** | Extend to height 10. Rollback to 7. `tip()` returns height 7. Heights 8-10 return `None` from `get_block_by_height()`. Blocks still accessible by hash. |
| **Rollback preserves fork blocks** | After rollback, `get_block(hash_of_height_9)` still returns the block. `get_record(hash_of_height_9).in_canonical_chain` is false. |
| **Rollback below min** | After pruning to height 5, rollback to height 3 returns `RollbackBelowMin`. |
| **Rollback above tip** | Rollback to height > tip returns `RollbackAboveTip`. |
| **find_common_ancestor()** | Build two forks diverging at height 5. Store both. `find_common_ancestor(fork_b_tip)` returns (hash_at_5, 5). |
| **apply_reorg()** | Build fork A (heights 1-10) and fork B (diverges at 5, heights 6'-10'). `apply_reorg(5, [6', 7', 8', 9', 10'])`. Verify result: reverted=[6..10], applied=[6'..10']. New tip is 10'. Canonical chain reflects fork B. |

### 17.5 Checkpoint Tests

| Category | Tests |
|----------|-------|
| **put + get checkpoint** | Store checkpoint for epoch 5. Retrieve. Verify all fields match. |
| **get_latest_checkpoint()** | Store checkpoints for epochs 1, 3, 5. Latest returns epoch 5. |
| **get_checkpoints_in_range()** | Store 5 checkpoints. Range [2, 4] returns 3. |
| **Idempotent checkpoint** | Re-storing same epoch replaces previous. |
| **Non-existent checkpoint** | `get_checkpoint(999)` returns `None`. |

### 17.6 Pruning Tests

| Category | Tests |
|----------|-------|
| **prune_before_height()** | Extend to height 20. Prune before 10. Heights 1-9 return `None`. Heights 10-20 still accessible. Returns count 9. |
| **Prune updates min_height** | After prune, `stats().min_height` reflects new minimum. |
| **Prune removes from cache** | Pruned blocks are evicted from LRU cache. |
| **prune_checkpoints_before_epoch()** | Store 5 checkpoints. Prune before epoch 3. Epochs 0-2 gone, 3-4 remain. |
| **Prune non-canonical blocks** | Fork blocks below prune height are also removed. |

### 17.7 Persistence Tests

| Category | Tests |
|----------|-------|
| **Crash recovery** | Store blocks, close store, reopen. All blocks, canonical chain, and tip are intact. |
| **Genesis persistence** | `init_genesis()`, close, reopen. Genesis block and tip at height 1 are intact. |
| **Schema version** | Open with correct version succeeds. Open with wrong version returns `SchemaMismatch`. |
| **Read-only mode** | Open readonly. Reads succeed. Writes return error. |

### 17.8 Performance Tests

| Category | Tests |
|----------|-------|
| **Sequential write throughput** | Insert 10,000 blocks sequentially. Measure blocks/second. |
| **Random read throughput** | Insert 10,000 blocks, random-access 1,000. Measure reads/second. |
| **Cache hit ratio** | Insert and access blocks with Zipf distribution. Verify cache hit rate > 80% for recent blocks. |
| **Range scan performance** | Insert 10,000 blocks. Scan range of 1,000. Measure scan time. |
| **Compression ratio** | Insert varied blocks. Verify average compression ratio 3-5x. |
| **Dictionary compression** | Train dictionary on 1000 blocks. Insert 1000 more. Verify dictionary-compressed ratio > plain zstd ratio by at least 15%. |
| **Write pipeline throughput** | Insert 10,000 blocks via `put_pipelined()`. Compare throughput to sequential `put()`. Expect 5-10x improvement. |
| **Mmap vs RocksDB lookup** | Compare `get_hash_by_height()` (mmap path) latency against direct `CF_CANONICAL` RocksDB read. Expect 100x+ improvement. |
| **Sharded cache contention** | Spawn 16 reader threads, each doing 10,000 cache lookups. Measure total throughput with 1 shard vs 16 shards. Expect ~10x improvement at 16 threads. |
| **Cache warming** | Open store with and without `warm_cache_on_open`. Measure first 100 random reads. Expect cache-warmed store to be significantly faster. |
| **Async vs sync throughput** | Compare `get_block_async()` vs `get_block()` under concurrent load from tokio tasks. |

### 17.9 Property Tests

| Property | Description |
|----------|-------------|
| **Round-trip identity** | For all block types: `get(put(x).hash) == x`. |
| **Canonical consistency** | For any height h in [min, tip]: `get_hash_by_height(h)` returns `Some` and the referenced block exists. |
| **Tip monotonicity** | After `extend_chain(b)` where `b.height > tip`, `tip().height == b.height`. |
| **Rollback monotonicity** | After `rollback_to_height(h)`, `tip().height == h` and heights > h are not canonical. |
| **Idempotency** | `put(x); put(x)` is equivalent to `put(x)`. State is identical. |
| **Prune safety** | After `prune_before_height(h)`, no block with height < h is accessible by height. Canonical chain from h to tip is intact. |
| **Fork isolation** | Non-canonical blocks never appear in height-indexed queries. |
| **Reorg atomicity** | `apply_reorg()` either fully completes or leaves state unchanged. |
| **Mmap consistency** | `canonical.bin` contents match `CF_CANONICAL` for all heights in [min, tip]. |
| **Dictionary fallback** | Blocks written before dictionary training can still be read after dictionary is loaded (plain zstd fallback). |

### 17.10 Integration Tests

| Test | Description |
|------|-------------|
| **Full lifecycle** | Create store, init genesis, extend chain through 3 epochs (96 blocks), store checkpoints, verify all queries return correct data. |
| **Reorg end-to-end** | Build canonical chain to height 50. Introduce fork at height 40 extending to height 55. `apply_reorg()`. Verify old blocks 41-50 are non-canonical but accessible. New blocks 41-55 are canonical. Tip is 55. |
| **Sync simulation** | Insert 1,000 blocks in batches (simulating sync from peer). Verify chain integrity at each batch. Serve blocks back via `get_blocks_in_range()` and `block_to_wire_bytes()`. |
| **Prune + continue** | Extend to height 1000. Prune before 500. Continue extending to 2000. Verify chain is intact from 500-2000. |
| **Concurrent reads** | Spawn 10 reader threads and 1 writer thread. Writer extends chain. Readers query by height and hash. Verify no panics or inconsistencies. |
| **Wire format parity** | Serialize block with bincode (storage format) and with `chia-traits::Streamable` (wire format). Deserialize both. Verify both produce identical `L2Block`. |
| **Epoch query consistency** | Extend through 5 epochs. For each epoch, `get_epoch_block_hashes(e)` returns exactly `BLOCKS_PER_EPOCH` hashes. Hashes match `get_hash_by_height()` for the epoch's height range. |
| **Write pipeline end-to-end** | Insert 5,000 blocks via `put_pipelined()` from 10 concurrent tasks. Verify all blocks persisted, canonical chain intact, no duplicates. |
| **Snapshot round-trip** | Extend to height 1000. `export_snapshot(1, 1000)` to file. Create new empty store. `import_snapshot()`. Verify all blocks, canonical chain, and tip match original. |
| **Snapshot integrity** | Tamper with one byte in exported snapshot. `import_snapshot()` returns checksum error. |
| **Mmap crash recovery** | Extend to height 100. Simulate crash by truncating `canonical.bin` to height 80. Reopen store. Verify it rebuilds mmap from `CF_CANONICAL` and chain is intact. |
| **BlobDB compaction** | Insert 10,000 blocks (BlobDB enabled). Trigger manual compaction. Verify blocks still readable. Verify blob garbage collection removes unreferenced blobs after pruning. |
| **Async concurrent stress** | 50 tokio tasks: 10 writers (`put_pipelined`), 20 readers by hash, 10 readers by height, 10 range scanners. Run for 5 seconds. Verify no panics, no data corruption. |
