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
| TYP-001 | gap | Column Family Constants | Assert each CF_* constant equals its expected string value. Verify all six constants are distinct. |
| TYP-002 | gap | Metadata Keys and RocksDB Defaults | Assert each META_* constant equals its expected string value. Assert each DEFAULT_* constant equals its expected numeric value. Verify SCHEMA_VERSION = 1. |
| TYP-003 | gap | Per-CF Configuration | Open a BlockStore, inspect RocksDB options for each CF. Verify compaction style, bloom filter presence, BlobDB settings, and compression settings match specification. |
| TYP-004 | gap | BlockRecord Struct | Construct BlockRecord via from_header() with a test L2BlockHeader. Verify all fields are populated correctly. Verify BlockRecord is not serialized to RocksDB (in-memory only). |
| TYP-005 | gap | StoredCheckpoint Struct | Construct a StoredCheckpoint with all fields populated. Verify serialization round-trip preserves all field values. |
| TYP-006 | gap | ChainTip Struct | Construct ChainTip, call to_bytes(), verify length is 40. Call from_bytes() on the output, verify round-trip equality. Test known hash+height pair against expected byte layout. |
| TYP-007 | gap | StorageStats Struct | Construct StorageStats::default(), verify all counts are 0 and Options are None. Construct with specific values and verify field access. |
| TYP-008 | gap | BlockStoreConfig Struct | Construct BlockStoreConfig::default(), verify every field matches the specified default value. Override individual fields and verify they take effect. |
