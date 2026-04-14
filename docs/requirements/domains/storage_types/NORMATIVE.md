# Storage Types - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Storage Types |
| **Prefix** | TYP |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### TYP-001: Column Family Constants

The crate **MUST** define the following column family name constants:

- `CF_BLOCKS = "blocks"` &mdash; Stores serialized L2Block data
- `CF_HEADERS = "headers"` &mdash; Stores serialized L2BlockHeader data
- `CF_ATTESTED = "attested"` &mdash; Stores serialized AttestedBlock data
- `CF_CANONICAL = "canonical"` &mdash; Maps height to canonical block hash
- `CF_CHECKPOINTS = "checkpoints"` &mdash; Stores serialized StoredCheckpoint data
- `CF_METADATA = "metadata"` &mdash; Stores key-value metadata (tip, genesis hash, etc.)

**Spec reference:** SPEC Section 2.1

---

### TYP-002: Metadata Keys and RocksDB Defaults

The crate **MUST** define the following metadata key constants:

- `META_TIP = "tip"` &mdash; Current chain tip (ChainTip encoding)
- `META_GENESIS_HASH = "genesis_hash"` &mdash; Genesis block hash (32 bytes)
- `META_MIN_HEIGHT = "min_height"` &mdash; Minimum retained height (8 bytes LE)
- `META_SCHEMA_VERSION = "schema_version"` &mdash; Schema version (8 bytes LE)
- `META_ZSTD_DICT = "zstd_dict"` &mdash; Trained zstd dictionary bytes
- `SCHEMA_VERSION = 1` &mdash; Current schema version number

The crate **MUST** define the following RocksDB tuning default constants:

- `DEFAULT_WRITE_BUFFER_SIZE = 67_108_864` (64 MB)
- `DEFAULT_BLOCK_CACHE_SIZE = 134_217_728` (128 MB)
- `DEFAULT_MAX_OPEN_FILES = 1000`
- `DEFAULT_BLOOM_BITS_PER_KEY = 10`
- `DEFAULT_BLOCK_CACHE_CAPACITY = 1000`
- `DEFAULT_HEADER_CACHE_CAPACITY = 2000`
- `ZSTD_COMPRESSION_LEVEL = 3`

**Spec reference:** SPEC Section 2.2, 2.3

---

### TYP-003: Per-CF Configuration

Each column family **MUST** be configured with the following RocksDB options:

- **CF_BLOCKS**: Universal compaction, BlobDB enabled with `min_blob_size = 512`, no bloom filter.
- **CF_HEADERS**: Level compaction, bloom filter with 10 bits per key, no compression.
- **CF_ATTESTED**: Level compaction, bloom filter with 10 bits per key.
- **CF_CANONICAL**: Level compaction, no bloom filter, no compression.
- **CF_CHECKPOINTS**: Level compaction with large target file size.
- **CF_METADATA**: Level compaction with default settings.

**Spec reference:** SPEC Section 2.4

---

### TYP-004: BlockRecord Struct

`BlockRecord` **MUST** contain the following fields organized into logical groups:

**Identity:**
- `hash: Bytes32`
- `height: u64`
- `epoch: u64`
- `parent_hash: Bytes32`

**Chain position:**
- `in_canonical_chain: bool`
- `status: BlockStatus`

**Statistics:**
- `timestamp: u64`
- `proposer_index: u32`
- `spend_bundle_count: u32`
- `total_cost: u64`
- `total_fees: u64`
- `additions_count: u32`
- `removals_count: u32`
- `block_size: u64`

**L1 anchor:**
- `l1_height: u32`
- `l1_hash: Bytes32`

**State:**
- `state_root: Bytes32`

`BlockRecord` **MUST** provide `from_header(header: &L2BlockHeader, status: BlockStatus) -> Self`.

`BlockRecord` is in-memory only and **MUST NOT** be persisted to RocksDB directly.

**Spec reference:** SPEC Section 3.2

---

### TYP-005: StoredCheckpoint Struct

`StoredCheckpoint` **MUST** contain the following fields:

- `checkpoint: Checkpoint`
- `signer_bitmap: SignerBitmap`
- `aggregate_signature: Signature`
- `aggregate_pubkey: PublicKey`
- `score: u64`
- `submitter: u32`
- `l1_height: Option<u32>`
- `l1_coin_id: Option<Bytes32>`
- `stored_at: u64`

**Spec reference:** SPEC Section 3.3

---

### TYP-006: ChainTip Struct

`ChainTip` **MUST** contain:

- `hash: Bytes32`
- `height: u64`

The binary encoding **MUST** be exactly 40 bytes: `hash` (32 bytes) concatenated with `height` (8 bytes, little-endian).

`ChainTip` **MUST** provide:
- `to_bytes(&self) -> [u8; 40]`
- `from_bytes(bytes: &[u8]) -> Result<Self>`

**Spec reference:** SPEC Section 3.4

---

### TYP-007: StorageStats Struct

`StorageStats` **MUST** contain the following fields:

- `block_count: u64`
- `canonical_block_count: u64`
- `header_count: u64`
- `checkpoint_count: u64`
- `attested_count: u64`
- `tip_height: Option<u64>`
- `min_height: Option<u64>`
- `total_size_bytes: u64`

`StorageStats` **MUST** derive `Default`.

**Spec reference:** SPEC Section 3.5

---

### TYP-008: BlockStoreConfig Struct

`BlockStoreConfig` **MUST** contain the following fields with the specified default values:

- `path: PathBuf` &mdash; (no default, required)
- `block_cache_capacity: usize` &mdash; default `1000`
- `header_cache_capacity: usize` &mdash; default `2000`
- `cache_shards: usize` &mdash; default `16`
- `warm_cache_on_open: bool` &mdash; default `true`
- `write_buffer_size: usize` &mdash; default `67_108_864` (64 MB)
- `block_cache_size: usize` &mdash; default `134_217_728` (128 MB)
- `max_open_files: i32` &mdash; default `1000`
- `enable_blob_db: bool` &mdash; default `true`
- `compress_blocks: bool` &mdash; default `true`
- `compression_level: i32` &mdash; default `3`
- `use_compression_dict: bool` &mdash; default `true`
- `write_pipeline_batch_size: usize` &mdash; default `64`
- `write_pipeline_flush_ms: u64` &mdash; default `100`
- `sync_writes: bool` &mdash; default `false`
- `enable_compaction_pruning: bool` &mdash; default `false`
- `min_retained_height: Option<u64>` &mdash; default `None`

`BlockStoreConfig` **MUST** provide a `Default` impl that sets all fields to the values listed above (except `path`, which **MUST** default to a reasonable value such as `"data/blockstore"`).

**Spec reference:** SPEC Section 3.6
