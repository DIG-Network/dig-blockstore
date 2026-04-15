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
| ROR-001 | done | Rollback to Height | `tests/ror_005_tests.rs` (validation + mutation): WriteBatch deletes on CF_CANONICAL, mmap truncate, tip update, record_cache in_canonical_chain=false, canonical_height_cache eviction. rollback_to_zero reverts chain correctly. 6 tests (shared with ROR-005). |
| ROR-002 | gap | Find Common Ancestor | Build a canonical chain, then store a fork branching at height F. Call find_common_ancestor with fork tip hash. Verify it returns the block at height F. Verify None is returned when max_depth is too small. Verify None for unknown hash. |
| ROR-003 | gap | Apply Reorg (Atomic) | Build a canonical chain of length N, store an alternate fork from height F. Call apply_reorg with ancestor_height=F and new fork hashes. Verify old canonical entries above F are removed, new entries are set, tip is updated. Verify ReorgResult counts. |
| ROR-004 | gap | Fork Preservation | After rollback_to_height, verify all rolled-back blocks are still retrievable by hash via get_block and get_header. After apply_reorg, verify blocks from the old fork are still in CF_BLOCKS and CF_HEADERS. |
| ROR-005 | done | Rollback Boundary Validation | `tests/ror_005_tests.rs` — NoTip on empty, RollbackAboveTip with target/tip values, at-tip no-op, no mutation on error, min_retained_height=0 when no pruning, rollback to 0 succeeds. 6 tests. |
| ROR-006 | done | Blocks to Revert | `tests/ror_006_tests.rs` — descending order, no-tip empty, at/above tip empty, to-zero returns all non-genesis, no mutation. 5 tests. |
