# Storage Types - Verification Matrix

| Field | Value |
|-------|-------|
| **Domain** | Storage Types |
| **Prefix** | TYP |
| **Normative** | [NORMATIVE.md](NORMATIVE.md) |
| **Tracking** | [TRACKING.yaml](TRACKING.yaml) |

---

## Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| TYP-001 | done | Column Family Constants | `tests/typ_001_cf_constants.rs` asserts exact TYP-001 strings, distinctness, `ALL_COLUMN_FAMILIES` equals exported set, and `BlockStore::open` smoke with those names. |
| TYP-002 | done | Metadata Keys and RocksDB Defaults | `tests/typ_002_metadata_keys.rs` asserts META_*, SCHEMA_VERSION, DEFAULT_*, ZSTD_COMPRESSION_LEVEL; `BlockStoreConfig::default()` matches TYP-002 numerics. |
| TYP-003 | done | Per-CF Configuration | `tests/typ_003_cf_config.rs` opens a temp DB, parses RocksDB `OPTIONS-*` dumps (`[CFOptions]` + `[TableOptions/BlockBasedTable]`) per family, asserts compaction/blob/bloom/compression/target_file_size match [`TYP-003.md`](specs/TYP-003.md). |
| TYP-004 | done | BlockRecord Struct | `tests/typ_004_block_record.rs` builds `L2BlockHeader` fixtures, asserts `BlockRecord::from_header` maps identity/stats/L1/state fields, `block_size == 0`, clone/eq/debug, and `in_canonical_chain` tracks `BlockStatus::is_canonical` (dig-block). Construction without `BlockStore` proves in-memory-only usage. |
| TYP-005 | done | StoredCheckpoint Struct | `tests/typ_005_stored_checkpoint.rs` exercises all nine fields, bincode round-trip (None/Some L1 fields), clone/debug, and a RocksDB `CF_CHECKPOINTS` put/get via `epoch_key` + `cf_options::column_family_descriptors`. |
| TYP-006 | done | ChainTip Struct | `tests/typ_006_chain_tip.rs` asserts 40-byte `to_bytes` layout (hash slice, height LE), `from_bytes` length errors via `Serialization`, round-trip, known vector (0xFF hash + height 42), height 0 / u64::MAX, and `Copy` semantics per [`TYP-006.md`](specs/TYP-006.md) test plan. |
| TYP-007 | done | StorageStats Struct | `tests/typ_007_storage_stats.rs` covers TYP-007 test plan (defaults, field assignment, clone/eq, Debug), plus explicit “no RocksDB” construction per acceptance. |
| TYP-008 | done | BlockStoreConfig Struct | `tests/typ_008_config.rs` asserts TYP-008 default table + extension defaults, structural override, `Clone`, and `Debug`; `path` is the RocksDB root ([`TYP-008.md`](specs/TYP-008.md)). |
