# Changelog

All notable changes to this project are documented here.
This project adheres to [Semantic Versioning](https://semver.org) and
[Conventional Commits](https://www.conventionalcommits.org).

## [0.1.3] - 2026-09-04

### Documentation
- Add CONTRIBUTING.md (#3)

## [0.1.2] - 2026-08-20

### Bug Fixes
- **ci:** Name this crate, not the one the workflows were copied from (#2)

## [0.1.1] - 2026-07-12

### Testing
- Add coverage-hardening suites for uncovered own-logic branches

### CI
- Enforce version increment in PRs (package.json / Cargo.toml)- Enforce Conventional Commits with commitlint on PRs- Enforce Conventional Commits with commitlint on PRs- Release automation (git-cliff changelog + tag on merge); publish is manual workflow_dispatch (#230)- Re-arm crates.io auto-publish on version tag (token in org secrets; auto-publish-everything #230)- Add flaky-test management (#489) (#1)

### Chores
- **changelog:** Add git-cliff config for Conventional-Commit changelog

## [0.1.0] - 2026-04-17

### Features
- **STR-001:** Cargo.toml DIG/Chia/storage dependencies and tests- **STR-002:** SPEC section 16 module hierarchy and tests- **STR-003:** Crate-root pub use for store, types, constants, encoding- **STR-004:** BlockStore open, open_readonly, init_genesis- **STR-005:** Shared test helpers and BlockStoreConfig expansion- **ERR-001:** BlockStoreError thirteen variants and tests- **ERR-002:** From<bincode::Error>, zstd compression_from_io, tests- **ERR-003:** Display message tests and error module docs- **TYP-001:** CF constants tests and spec-aligned docs- **TYP-002:** Metadata keys, SCHEMA_VERSION, RocksDB defaults- **TYP-003:** Per-CF RocksDB options and integration tests- **TYP-004:** BlockRecord with from_header and integration tests- **TYP-005:** StoredCheckpoint with bincode and CF_CHECKPOINTS test- **TYP-006:** Verify ChainTip 40-byte encoding with typ_006 tests- **TYP-007:** Implement StorageStats with dedicated typ_007 tests- **TYP-008:** Align BlockStoreConfig with spec defaults and path field- **KEY-003:** Epoch keys tests, docs, and tracking- **KEY-004:** Metadata key tests and encoding docs- **ser-001:** Block serialization with zstd dictionary support- **ser-002:** Header bincode serialization for CF_HEADERS- **ser-003:** Chia Streamable wire bytes for L2Block- **ser-004:** Round-trip guarantees tests and deserialize_block hash note- **ser-005:** Zstd dictionary training, persistence, and put path- **blk-001:** Put_block, record cache, and get_record- **blk-002:** Sharded block LRU and cache-first get_block- **blk-003:** Sharded header LRU and cache-first get_header- **blk-004:** Get_record cache layering and verification tests- **blk-005:** Get_blocks_by_hash with single multi_get_cf- **blockstore:** BLK-006 sequential stream with readahead- **blockstore:** BLK-007 async reads with spawn_blocking- **BLK-008:** Fix write-pipeline shutdown and complete verification- **BLK-009:** Put_attestation and get_attestation on CF_ATTESTED- **BLK-010:** Update_status for cached BlockRecord only- **BLK-011:** Has_block existence check with integration tests- **BLK-012:** BlockStore::stats with StorageStats- **BLK-013:** BlockStore flush and compact- **BLK-014:** Get_block_by_height and get_blocks_in_range- **BLK-015:** Get_record_by_height and get_records_in_range- **CAN-001:** Dual-layer canonical index (canonical.bin mmap + CF_CANONICAL)- **CAN-002:** CanonicalDenseFile mmap layout, growth, truncate- **CAN-003:** BlockStore::set_canonical for stored blocks- **CAN-004:** BlockStore::set_canonical_batch for reorg-scale promotion- **CAN-007:** Chain tip tracking — height(), set_tip() with CF_METADATA persistence- **CAN-006:** Get_hash_by_height with mmap hot path and CF_CANONICAL fallback- **CAN-005:** Extend_chain — primary block ingestion API- **CAC-001,CAC-002,CAC-003:** Formalize sharded block/header/record cache tests- **CAC-004,CAC-005:** Canonical height index cache + hash-to-height reverse cache- **CAC-006:** Cache warming on startup — populate ALL caches from canonical tip- **ROR-001,ROR-005,ROR-006:** Rollback_to_height + boundary validation + blocks_to_revert- **ROR-002:** Find_common_ancestor — parent_hash walk to canonical fork point- **ROR-003,ROR-004:** Apply_reorg atomic WriteBatch + fork preservation tests- **CKP-001..004:** Checkpoint storage — put, get, latest, range- **PRN-001,002,004,005:** Pruning — prune_before_height, checkpoints, min_retained, non-canonical- **SNP-001..004:** Snapshot export/import with SHA-256 checksum verification- **PRN-003:** Compaction filter on CF_HEADERS with shared Arc<AtomicU64> threshold- **API-001..005:** Public interface hardening — re-exports, cache reads, dead code removal

### Bug Fixes
- Update column_family_descriptors callers for new PRN-003 signature- **PRN-003:** Extend compaction filter to CF_BLOCKS and CF_ATTESTED; add ror_001_tests- **SER-003:** Use Streamable trait method for wire deserialization, not inherent from_bytes- **deps:** Resolve dig-block from crates.io instead of sibling path- **deps:** Switch dig-epoch to crates.io and drop stubs/dig-epoch

### Refactor
- **tests:** Flat tests layout; KEY-002 decode_height_key; Cursor rules- **MOD-001:** Extract compression logic from store.rs into compression.rs- **MOD-003:** Extract warm_caches into cache/warming.rs- **MOD-002:** Extract snapshot export/import into snapshot.rs- **MOD-005:** Extract canonical index logic into canonical/index.rs- **MOD-004:** Extract pipeline and streaming iterator into pipeline.rs

### Documentation
- **types:** Point StorageStats rustdoc at BlockStore::stats (BLK-012)- Comprehensive README with full public API reference

### Testing
- **KEY-001:** Add hash_key contract tests and complete tracking

### CI
- Add crate publishing

### Chores
- **tests:** Standardize requirement integration crates as <prefix>_<nnn>_tests.rs- Mark MOD-002/003/005 complete, MOD-004 remaining- Refresh GitNexus index stats in AGENTS.md / CLAUDE.md- Clean up clippy warnings and fix orphan doc comments


