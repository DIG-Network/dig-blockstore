# PRN-003: Compaction Filter

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 10.3 |

---

## Summary

When `enable_compaction_pruning` is enabled, a RocksDB compaction filter MUST automatically drop entries from `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED` where the associated block's height is below `min_retained_height`. The filter reads the threshold from a shared `AtomicU64`.

---

## Specification

### Compaction Filter Registration

When `BlockStoreConfig::enable_compaction_pruning` is `true`, the `BlockStore::open()` method MUST register a custom compaction filter on `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED`.

### Filter Decision Logic

For each key-value pair encountered during compaction:

1. Determine the block height associated with the entry:
   - For `CF_HEADERS`: deserialize the header and extract the height.
   - For `CF_BLOCKS` and `CF_ATTESTED`: the key is a block hash; a height lookup is needed (e.g., from a reverse index or by deserializing the associated header).
2. Read `min_retained_height` from the shared `AtomicU64` using `Ordering::Acquire`.
3. If `block_height < min_retained_height`, return `Decision::Remove` (drop the entry).
4. Otherwise, return `Decision::Keep`.

### Scope

- The compaction filter MUST only be applied to `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED`.
- The compaction filter MUST NOT be applied to `CF_CANONICAL`, `CF_CHECKPOINTS`, or `CF_METADATA`.

### Thread Safety

- The `AtomicU64` for `min_retained_height` MUST be shared between the main thread (which calls `prune_before_height`) and the compaction filter threads.
- Use `Arc<AtomicU64>` for shared ownership across threads.

---

## Acceptance Criteria

- [ ] Compaction filter is registered when `enable_compaction_pruning` is `true`
- [ ] Compaction filter is NOT registered when `enable_compaction_pruning` is `false`
- [ ] Filter drops entries from `CF_BLOCKS` where block height < `min_retained_height`
- [ ] Filter drops entries from `CF_HEADERS` where block height < `min_retained_height`
- [ ] Filter drops entries from `CF_ATTESTED` where block height < `min_retained_height`
- [ ] Filter does NOT affect `CF_CANONICAL`, `CF_CHECKPOINTS`, or `CF_METADATA`
- [ ] Filter reads `min_retained_height` from `AtomicU64` with `Acquire` ordering
- [ ] Filter is safe for concurrent use from compaction threads

---

## Implementation Notes

- RocksDB compaction filters run in background threads during compaction. They must be `Send + Sync`.
- The `CompactionFilter` trait in the `rocksdb` crate requires implementing `fn filter(&self, level: u32, key: &[u8], value: &[u8]) -> CompactionDecision`.
- For `CF_BLOCKS`, determining height from a block hash requires either: (a) deserializing the block header from the value, or (b) maintaining a separate hash-to-height index. Option (a) is simpler but slower during compaction. Consider caching or using a lightweight header prefix.
- The compaction filter acts as a secondary cleanup mechanism. The primary pruning path (`prune_before_height`) explicitly deletes entries. The compaction filter catches any entries that were missed (e.g., non-canonical blocks not tracked in `CF_CANONICAL`).

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_compaction_filter_enabled` | integration | Open with `enable_compaction_pruning=true`, verify filter is registered |
| `test_compaction_filter_disabled` | integration | Open with `enable_compaction_pruning=false`, verify no filter registered |
| `test_compaction_filter_drops_old_blocks` | integration | Store blocks, set min_retained_height, trigger manual compaction, verify old entries removed |
| `test_compaction_filter_keeps_new_blocks` | integration | Store blocks above min_retained_height, trigger compaction, verify entries retained |
| `test_compaction_filter_respects_cf_scope` | integration | Verify CF_CANONICAL and CF_METADATA entries are not dropped during compaction |
| `test_compaction_filter_atomic_threshold` | integration | Update AtomicU64 mid-compaction, verify new threshold is respected |

---

## Expected Test Files

- `tests/integration/pruning/test_prn_003_compaction_filter.rs`
