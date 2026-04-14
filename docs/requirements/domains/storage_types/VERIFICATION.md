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
| TYP-001 | done | Column Family Constants | `tests/storage_types/typ_001_cf_constants.rs` asserts exact TYP-001 strings, distinctness, `ALL_COLUMN_FAMILIES` equals exported set, and `BlockStore::open` smoke with those names. |
| TYP-002 | done | Metadata Keys and RocksDB Defaults | `tests/storage_types/typ_002_metadata_keys.rs` asserts META_*, SCHEMA_VERSION, DEFAULT_*, ZSTD_COMPRESSION_LEVEL; `BlockStoreConfig::default()` matches TYP-002 numerics. |
| TYP-003 | done | Per-CF Configuration | `tests/storage_types/typ_003_cf_config.rs` opens a temp DB, parses RocksDB `OPTIONS-*` dumps (`[CFOptions]` + `[TableOptions/BlockBasedTable]`) per family, asserts compaction/blob/bloom/compression/target_file_size match [`TYP-003.md`](specs/TYP-003.md). |
| TYP-004 | done | BlockRecord Struct | `tests/storage_types/typ_004_block_record.rs` builds `L2BlockHeader` fixtures, asserts `BlockRecord::from_header` maps identity/stats/L1/state fields, `block_size == 0`, clone/eq/debug, and `in_canonical_chain` tracks `BlockStatus::is_canonical` (dig-block). Construction without `BlockStore` proves in-memory-only usage. |
| TYP-005 | gap | StoredCheckpoint Struct | Construct a StoredCheckpoint with all fields populated. Verify serialization round-trip preserves all field values. |
| TYP-006 | gap | ChainTip Struct | Construct ChainTip, call to_bytes(), verify length is 40. Call from_bytes() on the output, verify round-trip equality. Test known hash+height pair against expected byte layout. |
| TYP-007 | gap | StorageStats Struct | Construct StorageStats::default(), verify all counts are 0 and Options are None. Construct with specific values and verify field access. |
| TYP-008 | gap | BlockStoreConfig Struct (19 fields including readahead_size and write_pipeline_channel_capacity) | Construct BlockStoreConfig::default(), verify every field matches the specified default value. Override individual fields and verify they take effect. |
