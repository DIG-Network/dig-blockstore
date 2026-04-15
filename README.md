# dig-blockstore

Persistent block storage for the DIG L2 network. RocksDB-backed with six column families, dual-layer canonical chain indexing (memory-mapped file + RocksDB), sharded in-memory LRU caches, zstd compression with trained dictionaries, async write pipeline, rollback/reorg support, checkpoint storage, pruning, and snapshot export/import.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      BlockStore                         │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────────┐ │
│  │ block    │  │ header   │  │ record cache          │ │
│  │ cache    │  │ cache    │  │ (HashMap, RAM-only)    │ │
│  │ (sharded │  │ (sharded │  │                       │ │
│  │  LRU)    │  │  LRU)    │  │ + height index cache  │ │
│  └────┬─────┘  └────┬─────┘  │ + hash→height cache   │ │
│       │              │        └───────────┬───────────┘ │
│  ┌────┴──────────────┴────────────────────┴───────────┐ │
│  │              RocksDB (6 column families)            │ │
│  │  CF_BLOCKS  CF_HEADERS  CF_CANONICAL  CF_METADATA  │ │
│  │  CF_ATTESTED  CF_CHECKPOINTS                       │ │
│  └────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────┐ │
│  │  canonical.bin (memory-mapped dense height→hash)   │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

```rust
use dig_blockstore::{BlockStore, BlockStoreConfig, ChainTip};

// Open or create a store
let config = BlockStoreConfig {
    path: "data/my-blockstore".into(),
    ..Default::default()
};
let store = BlockStore::open(config)?;

// Initialize genesis
store.init_genesis(&genesis_block)?;

// Extend the canonical chain
store.extend_chain(&block)?;

// Query by hash or height
let block = store.get_block(&hash)?;
let header = store.get_header_by_height(42)?;
let tip = store.tip(); // Option<ChainTip>
```

---

## Public API Reference

### BlockStore — Constructors

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `open(config)` | `BlockStoreConfig` | `Result<BlockStore>` | Open or create a store at `config.path`. Creates all column families, loads tip from metadata, optionally warms caches. |
| `open_readonly(path)` | `impl AsRef<Path>` | `Result<BlockStore>` | Open existing database read-only. All mutation methods return `ERR_MUTATION_READ_ONLY`. Path must exist. |

### BlockStore — Genesis & Chain Tip

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `init_genesis(&self, block)` | `&L2Block` | `Result<()>` | Initialize an empty store with the genesis block. Atomic WriteBatch writes block + header + canonical index + tip + genesis hash. Fails if already initialized. |
| `tip(&self)` | — | `Option<ChainTip>` | Current canonical chain tip (hash + height). `None` before genesis. Lock-free read from `RwLock`. |
| `height(&self)` | — | `Option<u64>` | Shorthand for `tip().map(\|t\| t.height)`. |
| `set_tip(&self, tip)` | `ChainTip` | `Result<()>` | Persist a new chain tip to `CF_METADATA`/`META_TIP` (40 bytes: hash \|\| height LE). Updates in-memory cache. |

### BlockStore — Block Storage (Write)

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `put_block(&self, block, canonical)` | `&L2Block, bool` | `Result<bool>` | Store a block. Returns `true` if novel, `false` if duplicate (idempotent). When `canonical=true`, also writes height→hash to `CF_CANONICAL`. Triggers dictionary training when block count reaches 1000. |
| `put(&self, block, canonical)` | `&L2Block, bool` | `Result<bool>` | Alias for `put_block`. |
| `extend_chain(&self, block)` | `&L2Block` | `Result<bool>` | Primary ingestion API: `put(block, true)` + `set_tip`. Returns `false` for duplicates. Does NOT validate parent-hash linkage (caller's responsibility). |
| `put_pipelined(&self, block, canonical)` | `L2Block, bool` | `Result<Receiver<Result<bool>>>` | Async batched write. Enqueues into bounded channel; background task flushes via single WriteBatch. Returns oneshot receiver for per-block ack. Requires tokio runtime. |

### BlockStore — Block Retrieval (Read)

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `get_block(&self, hash)` | `&Bytes32` | `Result<Option<L2Block>>` | Three-tier lookup: sharded LRU cache → RocksDB CF_BLOCKS (zstd decompress) → `None`. Populates cache on miss. |
| `get_header(&self, hash)` | `&Bytes32` | `Result<Option<L2BlockHeader>>` | Header cache → RocksDB CF_HEADERS (bincode, no zstd) → `None`. |
| `get_record(&self, hash)` | `&Bytes32` | `Result<Option<BlockRecord>>` | Record cache → header cache (derive) → CF_HEADERS (deserialize + derive) → `None`. Records are never persisted. |
| `has_block(&self, hash)` | `&Bytes32` | `Result<bool>` | Lightweight existence check. Probes caches first (no LRU promotion), then RocksDB key existence (no deserialization). |
| `get_blocks_by_hash(&self, hashes)` | `&[Bytes32]` | `Result<Vec<Option<L2Block>>>` | Batch retrieval. Cache hits served individually; all misses consolidated into one `multi_get_cf`. Output index matches input order. |

### BlockStore — Canonical Chain (Height-Based)

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `get_hash_by_height(&self, height)` | `u64` | `Result<Option<Bytes32>>` | Resolve canonical hash. Checks: BTreeMap cache → mmap `canonical.bin` → RocksDB `CF_CANONICAL`. Returns `None` for non-canonical or pruned heights. |
| `get_block_by_height(&self, height)` | `u64` | `Result<Option<L2Block>>` | `get_hash_by_height` → `get_block`. |
| `get_header_by_height(&self, height)` | `u64` | `Result<Option<L2BlockHeader>>` | `get_hash_by_height` → `get_header`. |
| `get_record_by_height(&self, height)` | `u64` | `Result<Option<BlockRecord>>` | `get_hash_by_height` → `get_record`. |
| `get_epoch_block_hashes(&self, epoch)` | `u64` | `Result<Vec<Bytes32>>` | Collect canonical hashes for all heights in the epoch. Stops at chain tip. Uses `dig_epoch::epoch_height_range`. |
| `set_canonical(&self, hash)` | `&Bytes32` | `Result<()>` | Mark an already-stored block as canonical at its height. Updates CF_CANONICAL + mmap + caches. |
| `set_canonical_batch(&self, hashes)` | `&[Bytes32]` | `Result<()>` | Atomic batch canonicalization via single WriteBatch. |

### BlockStore — Range Queries

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `get_blocks_in_range(&self, start, end)` | `u64, u64` | `Result<Vec<L2Block>>` | Eagerly load canonical blocks in `[start, end]` inclusive. Ascending order. Skips gaps. |
| `get_records_in_range(&self, start, end)` | `u64, u64` | `Result<Vec<BlockRecord>>` | Same range semantics but returns lightweight records (no zstd decompression). |
| `stream_blocks_in_range(&self, start, end)` | `u64, u64` | `StreamBlocksInRange` | Iterator with RocksDB readahead. Yields `Result<L2Block>` per height. Memory-efficient for large ranges. |

### BlockStore — Rollback & Reorg

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `blocks_to_revert(&self, target_height)` | `u64` | `Result<Vec<Bytes32>>` | **Read-only preview.** Returns canonical hashes that WOULD be reverted, descending height order. Empty if `target >= tip` or no tip. |
| `rollback_to_height(&self, target_height)` | `u64` | `Result<Vec<Bytes32>>` | Revert canonical chain. Validates: NoTip, RollbackAboveTip, RollbackBelowMin. Deletes CF_CANONICAL entries, truncates mmap, updates tip, marks records non-canonical. Block data preserved. Returns reverted hashes (descending). |
| `find_common_ancestor(&self, hash, max_depth)` | `&Bytes32, u64` | `Result<Option<(Bytes32, u64)>>` | Walk parent_hash chain backward. Returns first canonical ancestor as `(hash, height)`. `None` if not found within `max_depth` steps. |
| `apply_reorg(&self, ancestor_height, new_chain_hashes)` | `u64, &[Bytes32]` | `Result<ReorgResult>` | **Atomic fork-switch.** Single WriteBatch: delete old canonical entries above ancestor, write new entries, update tip. Post-commit: mmap truncate + rewrite, cache updates. |

### BlockStore — Attestation Storage

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `put_attestation(&self, hash, attested)` | `&Bytes32, &AttestedBlock` | `Result<()>` | Persist attested block in CF_ATTESTED. Overwrites existing. |
| `get_attestation(&self, hash)` | `&Bytes32` | `Result<Option<AttestedBlock>>` | Retrieve attestation by block hash. |

### BlockStore — Checkpoint Storage

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `put_checkpoint(&self, checkpoint)` | `&StoredCheckpoint` | `Result<()>` | Store checkpoint keyed by epoch (big-endian). Idempotent overwrite. |
| `get_checkpoint(&self, epoch)` | `u64` | `Result<Option<StoredCheckpoint>>` | Retrieve checkpoint by epoch. |
| `get_latest_checkpoint(&self)` | — | `Result<Option<StoredCheckpoint>>` | Reverse iterator on CF_CHECKPOINTS. Returns highest-epoch entry. |
| `get_checkpoints_in_range(&self, start, end)` | `u64, u64` | `Result<Vec<StoredCheckpoint>>` | Forward iterator. Range `[start, end]` inclusive. Ascending epoch order. |

### BlockStore — Snapshot Export/Import

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `export_snapshot(&self, start, end, writer)` | `u64, u64, &mut impl Write` | `Result<SnapshotManifest>` | Write manifest + length-prefixed compressed blocks + SHA-256 checksum. Uses pre-compressed bytes from CF_BLOCKS (no recompression). |
| `import_snapshot(&self, reader)` | `&mut impl Read` | `Result<SnapshotManifest>` | Read manifest, validate version, ingest blocks (height contiguity + parent link checks), verify trailing SHA-256 checksum. |

### BlockStore — Pruning

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `prune_before_height(&self, height)` | `u64` | `Result<usize>` | Delete blocks/headers/attestations/canonical entries below height. Includes non-canonical blocks. Single WriteBatch. Updates `min_retained_height`. Returns count pruned. |
| `prune_checkpoints_before_epoch(&self, epoch)` | `u64` | `Result<usize>` | Delete checkpoints with epoch < target. Returns count pruned. |
| `min_retained_height(&self)` | — | `Result<u64>` | Pruning floor. Returns 0 if no pruning has occurred. Fast (AtomicU64 read). |

### BlockStore — Statistics & Maintenance

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `stats(&self)` | — | `Result<StorageStats>` | Aggregate counts: block_count, canonical_block_count, header_count, checkpoint_count, attested_count, tip_height, min_height, total_size_bytes. |
| `flush(&self)` | — | `Result<()>` | Force WAL flush to disk. |
| `compact(&self)` | — | `Result<()>` | Trigger manual compaction on all column families. |

### BlockStore — Async Methods

| Method | Input | Output | Description |
|--------|-------|--------|-------------|
| `get_block_async(&self, hash)` | `&Bytes32` | `Result<Option<L2Block>>` | Cache hit served directly; miss dispatched to `spawn_blocking`. |
| `get_header_async(&self, hash)` | `&Bytes32` | `Result<Option<L2BlockHeader>>` | Same pattern as `get_block_async`. |
| `get_block_by_height_async(&self, height)` | `u64` | `Result<Option<L2Block>>` | Always `spawn_blocking` (height→hash uses I/O). |

---

## Data Types

### ChainTip

```rust
pub struct ChainTip {
    pub hash: Bytes32,   // 32-byte block identity hash
    pub height: u64,     // block height
}
```

- `to_bytes() -> [u8; 40]` — Encode: `hash[0..32] || height_LE[32..40]`
- `from_bytes(&[u8]) -> Result<Self>` — Decode from exactly 40 bytes

### BlockRecord

In-memory block metadata. Never persisted to RocksDB. Derived from headers via `from_header`.

```rust
pub struct BlockRecord {
    pub hash: Bytes32,
    pub height: u64,
    pub epoch: u64,
    pub parent_hash: Bytes32,
    pub in_canonical_chain: bool,  // from BlockStatus::is_canonical()
    pub status: BlockStatus,       // Validated | SoftFinalized | HardFinalized | Orphaned | Rejected | Pending
    pub timestamp: u64,
    pub proposer_index: u32,
    pub spend_bundle_count: u32,
    pub total_cost: u64,
    pub total_fees: u64,
    pub additions_count: u32,
    pub removals_count: u32,
    pub block_size: u64,           // 0 until filled by future API
    pub l1_height: u32,
    pub l1_hash: Bytes32,
    pub state_root: Bytes32,
}
```

### StorageStats

```rust
pub struct StorageStats {
    pub block_count: u64,            // all blocks including forks
    pub canonical_block_count: u64,  // main chain only
    pub header_count: u64,
    pub checkpoint_count: u64,
    pub attested_count: u64,
    pub tip_height: Option<u64>,
    pub min_height: Option<u64>,     // pruning floor
    pub total_size_bytes: u64,       // approximate disk footprint
}
```

### ReorgResult

```rust
pub struct ReorgResult {
    pub reverted: Vec<Bytes32>,  // removed from canonical (descending height)
    pub applied: Vec<Bytes32>,   // added to canonical (ascending height)
    pub new_tip: ChainTip,
}
```

### StoredCheckpoint

```rust
pub struct StoredCheckpoint {
    pub checkpoint: Checkpoint,           // epoch, state_root, block_root, counts, fees
    pub signer_bitmap: SignerBitmap,
    pub aggregate_signature: Signature,   // BLS G2
    pub aggregate_pubkey: PublicKey,       // BLS G1
    pub score: u64,
    pub submitter: u32,
    pub l1_height: Option<u32>,
    pub l1_coin_id: Option<Bytes32>,
    pub stored_at: u64,                   // Unix timestamp
}
```

### SnapshotManifest

```rust
pub struct SnapshotManifest {
    pub version: u32,         // current: 1
    pub start_height: u64,
    pub end_height: u64,
    pub block_count: u64,     // must equal end_height - start_height + 1
    pub state_root: Bytes32,
    pub checksum: Bytes32,    // SHA-256 of all block data
}
```

---

## Error Types

```rust
pub enum BlockStoreError {
    RocksDb(rocksdb::Error),                          // RocksDB operation failed
    Serialization(String),                             // bincode/structural failure
    Compression(String),                               // zstd compress/decompress failure
    BlockNotFound(Bytes32),                            // hash not in CF_BLOCKS
    CheckpointNotFound(u64),                           // no checkpoint at epoch
    BlockNotInStore(Bytes32),                          // hash not in store (chain operation)
    RollbackBelowMin { target: u64, min: u64 },       // target < min_retained_height
    RollbackAboveTip { target: u64, tip: u64 },       // target > current tip
    NoTip,                                             // no chain tip set
    SchemaMismatch { expected: u32, found: u32 },      // on-disk version mismatch
    NotInitialized,                                    // store has no genesis
    EmptyReorgChain,                                   // apply_reorg with empty hashes
    PipelineClosed,                                    // async channel closed
}
```

---

## Configuration

All fields of `BlockStoreConfig` with defaults:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | `PathBuf` | `"data/blockstore"` | RocksDB directory |
| `block_cache_capacity` | `usize` | `1000` | Max blocks in sharded LRU |
| `header_cache_capacity` | `usize` | `2000` | Max headers in sharded LRU |
| `cache_shards` | `usize` | `16` | Shard count (power of 2) |
| `warm_cache_on_open` | `bool` | `true` | Preload recent blocks on open |
| `warm_cache_depth` | `u64` | `64` | Heights to warm from tip |
| `write_buffer_size` | `usize` | `67_108_864` | RocksDB memtable (64 MiB) |
| `block_cache_size` | `usize` | `134_217_728` | RocksDB block cache (128 MiB) |
| `max_open_files` | `i32` | `1000` | File descriptor budget |
| `enable_blob_db` | `bool` | `true` | BlobDB for large block values |
| `compress_blocks` | `bool` | `true` | Application-level zstd |
| `compression_level` | `i32` | `3` | Zstd level |
| `use_compression_dict` | `bool` | `true` | Trained dictionary after 1000 blocks |
| `max_decompressed_block_bytes` | `usize` | `134_217_728` | Decompression bomb cap (128 MiB) |
| `zstd_dictionary_override` | `Option<Vec<u8>>` | `None` | Inject dictionary for tests |
| `write_pipeline_batch_size` | `usize` | `64` | Max blocks per WriteBatch flush |
| `write_pipeline_flush_ms` | `u64` | `100` | Partial batch flush timer |
| `write_pipeline_channel_capacity` | `usize` | `256` | Bounded async channel depth |
| `sync_writes` | `bool` | `false` | WAL sync per write |
| `readahead_size` | `usize` | `2_097_152` | Sequential readahead hint (2 MiB) |
| `canonical_height_cache_capacity` | `usize` | `10_000` | Height→hash BTreeMap entries |
| `hash_to_height_cache_capacity` | `usize` | `10_000` | Hash→height reverse LRU entries |
| `enable_compaction_pruning` | `bool` | `false` | Register compaction filter |
| `min_retained_height` | `Option<u64>` | `None` | Pruning floor (config override) |

---

## Column Families

| Name | Key | Value | Purpose |
|------|-----|-------|---------|
| `blocks` | `hash (32 bytes)` | `zstd-compressed bincode L2Block` | Full block bodies |
| `headers` | `hash (32 bytes)` | `bincode L2BlockHeader` | Block headers (no compression) |
| `canonical` | `height (8 bytes BE)` | `hash (32 bytes)` | Canonical chain height→hash index |
| `metadata` | `UTF-8 string` | `varies` | Tip, genesis hash, schema version, zstd dict, min height |
| `checkpoints` | `epoch (8 bytes BE)` | `bincode StoredCheckpoint` | Consensus checkpoints by epoch |
| `attested` | `hash (32 bytes)` | `bincode AttestedBlock` | Attested block data |

---

## Key Encoding Functions

| Function | Input | Output | Description |
|----------|-------|--------|-------------|
| `hash_key(hash)` | `&Bytes32` | `&[u8; 32]` | Raw 32-byte hash for CF_BLOCKS/CF_HEADERS/CF_ATTESTED keys |
| `height_key(height)` | `u64` | `[u8; 8]` | Big-endian 8-byte key for CF_CANONICAL (preserves sort order) |
| `decode_height_key(key)` | `&[u8; 8]` | `u64` | Decode big-endian height key |
| `epoch_key(epoch)` | `u64` | `[u8; 8]` | Big-endian 8-byte key for CF_CHECKPOINTS |
| `decode_epoch_key(key)` | `&[u8; 8]` | `u64` | Decode big-endian epoch key |
| `metadata_key(name)` | `&str` | `&[u8]` | UTF-8 bytes for CF_METADATA keys |

---

## Wire Format (Chia Streamable)

| Function | Input | Output | Description |
|----------|-------|--------|-------------|
| `block_to_wire_bytes(block)` | `&L2Block` | `Result<Vec<u8>>` | Chia Streamable encoding (big-endian, length-prefixed lists). NOT bincode. |
| `block_from_wire_bytes(bytes)` | `&[u8]` | `Result<L2Block>` | Decode Streamable wire format. Rejects trailing garbage. |

---

## Snapshot Stream Format

```
[ SnapshotManifest (bincode) ]
[ block_0_len: u32 LE ] [ block_0_compressed_bytes ]
[ block_1_len: u32 LE ] [ block_1_compressed_bytes ]
...
[ SHA-256 checksum: 32 bytes ]
```

The checksum covers all bytes from the start of the manifest through the last block (exclusive of the trailing 32-byte checksum itself). Computed via `chia_sha2::Sha256`.

---

## Crate Dependencies

| Crate | Purpose |
|-------|---------|
| `dig-block` | L2Block, L2BlockHeader, BlockStatus, AttestedBlock, Checkpoint types |
| `dig-epoch` | Epoch height range calculation |
| `dig-constants` | Network constants |
| `chia-protocol` | Bytes32 fixed-size hash type |
| `chia-bls` | BLS Signature, PublicKey for checkpoint storage |
| `chia-sha2` | SHA-256 for snapshot checksums |
| `chia-traits` | Streamable trait for wire format |
| `rocksdb` | Storage backend with column families, WriteBatch, iterators |
| `zstd` | Block body compression with optional trained dictionary |
| `bincode` | Fast binary serialization for headers, checkpoints, blocks |
| `serde` | Serialization framework (derive) |
| `thiserror` | Error type derivation |
| `parking_lot` | Fast RwLock/Mutex (no poisoning) |
| `lru` | LRU cache implementation |
| `tokio` | Async runtime for write pipeline and async read APIs |
| `memmap2` | Memory-mapped canonical.bin file |
