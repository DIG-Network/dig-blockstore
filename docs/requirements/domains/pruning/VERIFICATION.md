# Pruning - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| PRN-001 | gap | Prune blocks before height: delete from CF_BLOCKS, CF_HEADERS, CF_ATTESTED, CF_CANONICAL; evict caches; WriteBatch atomicity; return count | Integration: store 10 canonical blocks, prune before height 5, verify 5 removed from all CFs; verify caches evicted; verify count returned; verify remaining blocks intact |
| PRN-002 | gap | Prune checkpoints before epoch: iterate and delete from CF_CHECKPOINTS; return count | Unit: store checkpoints at epochs 1-10, prune before epoch 5, verify epochs 1-4 removed; verify epochs 5-10 remain; verify count returned |
| PRN-003 | gap | Compaction filter drops entries from CF_BLOCKS, CF_HEADERS, CF_ATTESTED below min_retained_height | Integration: enable compaction pruning; set min_retained_height; trigger compaction; verify entries below threshold removed; verify CF_CANONICAL and CF_METADATA untouched |
| PRN-004 | gap | min_retained_height persisted in CF_METADATA; updated by prune_before_height; loaded on startup | Integration: prune and verify META_MIN_HEIGHT updated; reopen BlockStore and verify AtomicU64 initialized from persisted value; default 0 on fresh store |
| PRN-005 | gap | Non-canonical blocks at pruned heights also removed during pruning | Integration: store canonical and non-canonical blocks at same heights; prune before height; verify both canonical and non-canonical blocks removed; verify blocks above height retained |
