# Rollback & Reorg - Verification Matrix

| Field | Value |
|-------|-------|
| **Domain** | Rollback & Reorg |
| **Prefix** | ROR |
| **Normative** | [NORMATIVE.md](NORMATIVE.md) |
| **Tracking** | [TRACKING.yaml](TRACKING.yaml) |

---

## Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| ROR-001 | gap | Rollback to Height | Build a chain of N blocks, rollback to height M < N. Verify CF_CANONICAL has no entries above M. Verify mmap file size is `(M+1)*32`. Verify tip is updated to block at M. Verify reverted hashes are returned. Verify blocks still exist in CF_BLOCKS. |
| ROR-002 | gap | Find Common Ancestor | Build a canonical chain, then store a fork branching at height F. Call find_common_ancestor with fork tip hash. Verify it returns the block at height F. Verify None is returned when max_depth is too small. Verify None for unknown hash. |
| ROR-003 | gap | Apply Reorg (Atomic) | Build a canonical chain of length N, store an alternate fork from height F. Call apply_reorg with ancestor_height=F and new fork hashes. Verify old canonical entries above F are removed, new entries are set, tip is updated. Verify ReorgResult counts. Simulate crash mid-reorg (kill before post-commit) and verify RocksDB state is consistent. |
| ROR-004 | gap | Fork Preservation | After rollback_to_height, verify all rolled-back blocks are still retrievable by hash via get_block and get_header. After apply_reorg, verify blocks from the old fork are still in CF_BLOCKS and CF_HEADERS. |
| ROR-005 | gap | Rollback Boundary Validation | Test rollback_to_height with target > tip returns RollbackAboveTip. Test with target < min_retained_height returns RollbackBelowMin. Test with no tip set returns NoTip. Verify no mutations occur on validation failure. |
| ROR-006 | gap | Blocks to Revert | Build a canonical chain of length N. Call blocks_to_revert(M) where M < N. Verify returned hashes match canonical blocks at heights M+1..N in descending order. Verify no state mutations occur. Verify empty vec when target >= tip or no tip set. |
