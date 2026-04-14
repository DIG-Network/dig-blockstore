# Checkpoint Storage - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| CKP-001 | gap | Store checkpoint via bincode to CF_CHECKPOINTS keyed by big-endian epoch; idempotent overwrite | Unit: round-trip put/get; verify bincode serialization; overwrite existing epoch and verify latest value returned; verify big-endian key encoding |
| CKP-002 | gap | Get checkpoint by epoch from CF_CHECKPOINTS with bincode deserialization | Unit: retrieve stored checkpoint; verify correct deserialization; missing epoch returns None |
| CKP-003 | gap | Get latest checkpoint via reverse iterator on CF_CHECKPOINTS | Unit: store multiple checkpoints at different epochs, verify highest epoch returned; empty store returns None; add higher epoch, verify updated result |
| CKP-004 | gap | Get checkpoints in epoch range via forward iterator seek | Unit: store checkpoints at epochs 5,10,15,20; query range [8,18] returns epochs 10,15; empty range returns empty vec; boundary-inclusive test |
