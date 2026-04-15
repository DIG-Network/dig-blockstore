# Pruning - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| PRN-001 | done | Prune blocks before height: delete from CF_BLOCKS, CF_HEADERS, CF_ATTESTED, CF_CANONICAL; evict caches; WriteBatch atomicity; return count | `tests/prn_001_tests.rs` — canonical entries removed, block data removed, count returned, no-op below min, min_retained updated. 5 tests. |
| PRN-002 | done | Prune checkpoints before epoch: iterate and delete from CF_CHECKPOINTS; return count | `tests/prn_002_tests.rs` — basic pruning, zero epoch noop, none below target. 3 tests. |
| PRN-003 | done | Compaction filter on CF_HEADERS drops entries below min_retained_height during compaction | `tests/prn_003_tests.rs` — filter registered when enabled, not when disabled, drops below threshold, keeps above, threshold survives reopen. Shared Arc<AtomicU64>. 5 tests. |
| PRN-004 | done | min_retained_height persisted in CF_METADATA; updated by prune_before_height; loaded on startup | `tests/prn_004_tests.rs` — fresh=0, survives reopen, monotonically non-decreasing. 3 tests. |
| PRN-005 | done | Non-canonical blocks at pruned heights also removed during pruning | `tests/prn_005_tests.rs` — non-canonical pruned, above-height retained, canonical unaffected. 3 tests. |
