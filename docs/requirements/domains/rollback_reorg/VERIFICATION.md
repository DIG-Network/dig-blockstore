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
| ROR-001 | done | Rollback to Height | `tests/ror_001_tests.rs` — canonical entries removed, tip updated, block data preserved, records marked non-canonical, at-tip noop, descending order. `tests/ror_005_tests.rs` — boundary validation. 12 tests total. |
| ROR-002 | done | Find Common Ancestor | `tests/ror_002_tests.rs` — fork point detection, already-canonical returns self, unknown hash None, exceeds max_depth None, zero depth None, genesis as ancestor, broken parent chain None. 7 tests. |
| ROR-003 | done | Apply Reorg (Atomic) | `tests/ror_003_tests.rs` — single WriteBatch: delete old canonical + put new canonical + update tip. ReorgResult counts, empty chain error, missing block error, no-tip error, old blocks preserved. 6 tests. |
| ROR-004 | done | Fork Preservation | `tests/ror_004_tests.rs` — blocks survive rollback, headers survive rollback, non-canonical get_block, recanonicalize without re-store, block_count unchanged. 5 tests. |
| ROR-005 | done | Rollback Boundary Validation | `tests/ror_005_tests.rs` — NoTip on empty, RollbackAboveTip with target/tip values, at-tip no-op, no mutation on error, min_retained_height=0 when no pruning, rollback to 0 succeeds. 6 tests. |
| ROR-006 | done | Blocks to Revert | `tests/ror_006_tests.rs` — descending order, no-tip empty, at/above tip empty, to-zero returns all non-genesis, no mutation. 5 tests. |
