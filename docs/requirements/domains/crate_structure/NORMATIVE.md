# Crate Structure - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Crate Structure |
| **Prefix** | STR |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### STR-001: Cargo.toml Dependencies

Cargo.toml **MUST** declare the following dependencies with the specified minimum versions:

- `dig-block = "0.1"`
- `dig-epoch = "0.1"`
- `dig-constants = "0.1"`
- `chia-protocol = "0.26"`
- `chia-bls = "0.26"`
- `chia-sha2 = "0.26"`
- `chia-traits = "0.26"`
- `rocksdb`
- `zstd`
- `bincode`
- `serde` (with `derive` feature)
- `thiserror`
- `parking_lot`
- `lru`
- `tokio` (with `full` feature)
- `memmap2`

**Spec reference:** SPEC Section 1.2

---

### STR-002: Module Hierarchy

The module hierarchy **MUST** include the following source files and directories:

- `store.rs` &mdash; `BlockStore` struct and core implementation
- `config.rs` &mdash; `BlockStoreConfig` and configuration logic
- `types/` directory containing:
  - `block_record.rs` &mdash; `BlockRecord` struct
  - `stored_checkpoint.rs` &mdash; `StoredCheckpoint` struct
  - `chain_tip.rs` &mdash; `ChainTip` struct
  - `storage_stats.rs` &mdash; `StorageStats` struct
- `constants.rs` &mdash; Column family and metadata key constants
- `error.rs` &mdash; `BlockStoreError` enum
- `encoding.rs` &mdash; Key encoding functions
- `cache/` directory containing:
  - `sharded.rs` &mdash; Sharded LRU cache implementation
  - `warming.rs` &mdash; Cache warming on startup
- `canonical/` directory containing:
  - `index.rs` &mdash; Canonical chain index logic
  - `mmap.rs` &mdash; Memory-mapped canonical.bin file
- `compression.rs` &mdash; Zstd compression/decompression
- `pipeline.rs` &mdash; Async write pipeline
- `snapshot.rs` &mdash; Snapshot export/import

**Spec reference:** SPEC Section 16

---

### STR-003: Public Re-exports

`lib.rs` **MUST** re-export the following public API surface:

- `BlockStore`
- `BlockStoreConfig`
- `BlockRecord`
- `StoredCheckpoint`
- `ChainTip`
- `StorageStats`
- `BlockStoreError`
- All `CF_*` constants (`CF_BLOCKS`, `CF_HEADERS`, `CF_ATTESTED`, `CF_CANONICAL`, `CF_CHECKPOINTS`, `CF_METADATA`)
- All `META_*` constants (`META_TIP`, `META_GENESIS_HASH`, `META_MIN_HEIGHT`, `META_SCHEMA_VERSION`, `META_ZSTD_DICT`)
- Key encoding functions

**Spec reference:** SPEC Section 15

---

### STR-004: BlockStore Constructor

`BlockStore` **MUST** provide the following constructors and initialization methods:

- `open(BlockStoreConfig) -> Result<Self>` &mdash; Opens or creates the RocksDB store, creates all column families, loads the current tip from metadata, and optionally warms the cache if configured.
- `open_readonly(path) -> Result<Self>` &mdash; Opens an existing store in read-only mode.
- `init_genesis(&L2Block) -> Result<()>` &mdash; Stores the genesis block, sets the chain tip to height 0, and records the genesis hash in metadata.

**Spec reference:** SPEC Section 15.1

---

### STR-005: Test Infrastructure

Test infrastructure **MUST** include the following helpers:

- A helper to create a temporary RocksDB directory that is automatically cleaned up.
- A helper to create test `L2Block` and `L2BlockHeader` instances with deterministic hashes.
- A test `BlockStoreConfig` with small cache capacities suitable for unit testing.
- A helper to build a chain of N linked blocks with correct `parent_hash` chaining.

**Spec reference:** SPEC Section 17
