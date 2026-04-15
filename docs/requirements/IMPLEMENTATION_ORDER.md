# Implementation Order

Phased checklist for dig-blockstore requirements. Work top-to-bottom within each phase.
After completing a requirement: write tests, verify they pass, update TRACKING.yaml, VERIFICATION.md, and check off here.

**A requirement is NOT complete until comprehensive tests verify it.**

---

## Phase 0: Crate Structure & Foundation

- [x] STR-001 — Cargo.toml with DIG/Chia/storage crate dependencies and metadata
- [x] STR-002 — Module hierarchy (src/lib.rs root, submodule layout matching SPEC Section 16)
- [x] STR-003 — Public re-exports (BlockStore, BlockStoreConfig, BlockRecord, etc.)
- [x] STR-004 — BlockStore constructor (open, open_readonly, init_genesis)
- [x] STR-005 — Test infrastructure (temp RocksDB, test blocks, test config)

## Phase 1: Error Types & Constants

- [x] ERR-001 — BlockStoreError enum with all variants
- [x] ERR-002 — Error From conversions (rocksdb::Error, serialization, compression)
- [x] ERR-003 — Error context and Display messages

## Phase 2: Storage Types

- [x] TYP-001 — Column family constants (CF_BLOCKS, CF_HEADERS, CF_ATTESTED, CF_CANONICAL, CF_CHECKPOINTS, CF_METADATA)
- [x] TYP-002 — Metadata key constants and RocksDB tuning defaults
- [x] TYP-003 — Per-CF configuration (bloom filters, compression, compaction, BlobDB)
- [x] TYP-004 — BlockRecord struct with from_header() constructor
- [x] TYP-005 — StoredCheckpoint struct
- [x] TYP-006 — ChainTip struct with 40-byte encoding
- [x] TYP-007 — StorageStats struct
- [x] TYP-008 — BlockStoreConfig struct with all fields and defaults

## Phase 3: Key Encoding

- [x] KEY-001 — Hash keys (32 bytes, raw Bytes32)
- [x] KEY-002 — Height keys (8 bytes, big-endian u64, ascending sort)
- [x] KEY-003 — Epoch keys (8 bytes, big-endian u64)
- [x] KEY-004 — Metadata keys (variable-length UTF-8)

## Phase 4: Serialization & Compression

- [x] SER-001 — Block serialization with zstd dictionary compression
- [x] SER-002 — Header serialization (bincode, uncompressed)
- [x] SER-003 — Wire-format interop (chia-traits Streamable export/import)
- [x] SER-004 — Round-trip guarantees (bincode, zstd, hash invariance)
- [x] SER-005 — Dictionary training and management (train on 1000 blocks, persist, fallback)

## Phase 5: Block Storage

- [x] BLK-001 — put_block (store block + header + record, idempotent)
- [x] BLK-002 — get_block by hash (cache → RocksDB, decompress)
- [x] BLK-003 — get_header by hash (cache → RocksDB)
- [x] BLK-004 — get_record by hash (in-memory cache, derive from header on miss)
- [x] BLK-005 — Batch retrieval (get_blocks_by_hash via multi_get)
- [x] BLK-006 — Prefetching for sequential access (readahead)
- [x] BLK-007 — Async API (cache hit on executor, DB miss to spawn_blocking)
- [ ] BLK-008 — Write pipeline (async put_pipelined returning oneshot::Receiver)
- [ ] BLK-009 — put_attestation / get_attestation in CF_ATTESTED
- [ ] BLK-010 — update_status (BlockStatus update on BlockRecord in cache)
- [ ] BLK-011 — has_block (lightweight existence check by hash)
- [ ] BLK-012 — stats() (storage statistics via StorageStats)
- [ ] BLK-013 — flush() and compact() (WAL flush and manual compaction)
- [ ] BLK-014 — get_blocks_in_range (canonical blocks in [start, end] inclusive)
- [ ] BLK-015 — get_records_in_range (canonical records in [start, end] inclusive)

## Phase 6: Canonical Chain

- [ ] CAN-001 — Dual-layer canonical index (mmap hot path + CF_CANONICAL cold path)
- [ ] CAN-002 — canonical.bin memory-mapped file (dense array of 32-byte hashes)
- [ ] CAN-003 — set_canonical (mark existing block as canonical, update index)
- [ ] CAN-004 — set_canonical_batch (batch marking for reorg)
- [ ] CAN-005 — extend_chain (store block + update canonical + update tip atomically)
- [ ] CAN-006 — get_hash_by_height (O(1) mmap lookup, fallback to CF_CANONICAL)
- [ ] CAN-007 — Chain tip tracking (tip, height, set_tip, atomic updates)

## Phase 7: Caching

- [ ] CAC-001 — Sharded block cache (16 shards, configurable capacity)
- [ ] CAC-002 — Sharded header cache
- [ ] CAC-003 — BlockRecord cache (in-memory only, derive on miss)
- [ ] CAC-004 — Canonical height index cache
- [ ] CAC-005 — Hash-to-height reverse lookup cache
- [ ] CAC-006 — Cache warming on startup (preload recent blocks/headers)

## Phase 8: Rollback & Reorg

- [ ] ROR-001 — rollback_to_height (revert canonical without deleting blocks)
- [ ] ROR-002 — find_common_ancestor (walk parent hashes up to max_depth)
- [ ] ROR-003 — apply_reorg (atomic WriteBatch: rollback + set canonical for new chain)
- [ ] ROR-004 — Fork preservation (non-canonical blocks remain accessible by hash)
- [ ] ROR-005 — Rollback boundary validation (below genesis, above tip)
- [ ] ROR-006 — blocks_to_revert (read-only revert preview)

## Phase 9: Checkpoint Storage

- [ ] CKP-001 — put_checkpoint by epoch (StoredCheckpoint in CF_CHECKPOINTS)
- [ ] CKP-002 — get_checkpoint by epoch
- [ ] CKP-003 — get_latest_checkpoint
- [ ] CKP-004 — get_checkpoints_in_range (epoch range query)

## Phase 10: Pruning

- [ ] PRN-001 — prune_before_height (remove blocks/headers/records/attestations)
- [ ] PRN-002 — prune_checkpoints_before_epoch
- [ ] PRN-003 — Compaction filter (background pruning during RocksDB compaction)
- [ ] PRN-004 — min_retained_height tracking (update on prune, persist in CF_METADATA)
- [ ] PRN-005 — Non-canonical block pruning

## Phase 11: Snapshot

- [ ] SNP-001 — Export snapshot (canonical blocks [start,end], streaming with manifest)
- [ ] SNP-002 — Import snapshot (validate schema, contiguity, parent-child links)
- [ ] SNP-003 — SnapshotManifest struct
- [ ] SNP-004 — Checksum verification (SHA-256 of all preceding bytes)

---

## Summary

| Phase | Domain(s) | Requirements |
|-------|-----------|-------------|
| 0 | Crate Structure | STR-001 — STR-005 (5) |
| 1 | Error Types | ERR-001 — ERR-003 (3) |
| 2 | Storage Types | TYP-001 — TYP-008 (8) |
| 3 | Key Encoding | KEY-001 — KEY-004 (4) |
| 4 | Serialization | SER-001 — SER-005 (5) |
| 5 | Block Storage | BLK-001 — BLK-015 (15) |
| 6 | Canonical Chain | CAN-001 — CAN-007 (7) |
| 7 | Caching | CAC-001 — CAC-006 (6) |
| 8 | Rollback & Reorg | ROR-001 — ROR-006 (6) |
| 9 | Checkpoint Storage | CKP-001 — CKP-004 (4) |
| 10 | Pruning | PRN-001 — PRN-005 (5) |
| 11 | Snapshot | SNP-001 — SNP-004 (4) |
| **Total** | | **72** |
