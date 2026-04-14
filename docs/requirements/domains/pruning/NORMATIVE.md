# Pruning - Normative Requirements

- **Domain:** pruning
- **Prefix:** PRN
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### PRN-001: Prune Before Height (`prune_before_height`)

`prune_before_height(&self, height: u64) -> Result<usize>`

1. MUST iterate `CF_CANONICAL` from `min_height` to `height` (exclusive) and collect all block hashes at those heights.
2. MUST delete the corresponding entries from `CF_BLOCKS`, `CF_HEADERS`, `CF_ATTESTED`, and `CF_CANONICAL` for each collected block hash.
3. MUST evict pruned blocks, headers, and records from their respective in-memory caches.
4. MUST use a RocksDB `WriteBatch` for atomicity of all deletions.
5. MUST update `min_retained_height` in `CF_METADATA` under the `META_MIN_HEIGHT` key.
6. MUST return the count of pruned blocks.

**Spec reference:** 10.1

---

### PRN-002: Prune Checkpoints Before Epoch (`prune_checkpoints_before_epoch`)

`prune_checkpoints_before_epoch(&self, epoch: u64) -> Result<usize>`

1. MUST iterate `CF_CHECKPOINTS` from epoch 0 to `epoch` (exclusive) and delete each entry.
2. MUST return the count of pruned checkpoints.

**Spec reference:** 10.2

---

### PRN-003: Compaction Filter

1. When `enable_compaction_pruning` is `true`, a RocksDB compaction filter MUST be registered that drops entries from `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED` where the associated block's height is less than `min_retained_height`.
2. The compaction filter MUST read `min_retained_height` from a shared `AtomicU64` to determine the current pruning threshold.
3. The compaction filter MUST NOT drop entries from column families other than `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED`.

**Spec reference:** 10.3

---

### PRN-004: min_retained_height Tracking

1. `min_retained_height` MUST be persisted in `CF_METADATA` under the `META_MIN_HEIGHT` key.
2. `prune_before_height()` MUST update this value after successful pruning.
3. On startup, the stored `min_retained_height` MUST be read from `CF_METADATA` to initialize the `AtomicU64` used by the compaction filter.
4. If no value exists in `CF_METADATA` on startup, `min_retained_height` MUST default to 0.

**Spec reference:** 10.4

---

### PRN-005: Non-Canonical Block Pruning

1. Pruning MUST also remove non-canonical blocks at pruned heights.
2. When iterating `CF_BLOCKS` during pruning, blocks whose height (obtained from header lookup in `CF_HEADERS`) is less than the target pruning height MUST be removed regardless of canonical status.
3. Associated entries in `CF_HEADERS` and `CF_ATTESTED` for non-canonical blocks MUST also be deleted.

**Spec reference:** 10.1
