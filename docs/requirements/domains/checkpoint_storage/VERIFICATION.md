# Checkpoint Storage - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| CKP-001 | done | Store checkpoint via bincode to CF_CHECKPOINTS keyed by big-endian epoch; idempotent overwrite | `tests/ckp_001_tests.rs` — round-trip, overwrite, multiple epochs. 3 tests. |
| CKP-002 | done | Get checkpoint by epoch from CF_CHECKPOINTS with bincode deserialization | `tests/ckp_002_tests.rs` — existing, missing, no cross-contamination. 3 tests. |
| CKP-003 | done | Get latest checkpoint via reverse iterator on CF_CHECKPOINTS | `tests/ckp_003_tests.rs` — none empty, highest epoch, updates after new insert. 3 tests. |
| CKP-004 | done | Get checkpoints in epoch range via forward iterator seek | `tests/ckp_004_tests.rs` — inclusive range, empty, inverted range, single, gaps. 5 tests. |
