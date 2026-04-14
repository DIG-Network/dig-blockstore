# Key Encoding - Verification Matrix

| Req ID | Requirement | Verification Method | Test File(s) | Status |
|--------|-------------|-------------------|--------------|--------|
| KEY-001 | Hash keys MUST be raw 32-byte Bytes32 values with no prefix | Unit test: `hash_key` length, byte identity, round-trip `Bytes32`, distinct inputs | `tests/test_key_001_hash_keys.rs` | Done |
| KEY-002 | Height keys MUST be u64 big-endian 8-byte encoding with correct sort order | Unit test: `height_key`/`decode_height_key`, table vectors, sort invariant, LE counterexample | `tests/test_key_002_height_keys.rs` | Done |
| KEY-003 | Epoch keys MUST be u64 big-endian 8-byte encoding | Unit test: `epoch_key`/`decode_epoch_key`, zero/one/max, sort table, round-trip, parity with `height_key` | `tests/test_key_003_epoch_keys.rs` | Done |
| KEY-004 | Metadata keys MUST be UTF-8 encoded strings | Unit test: encode metadata keys, assert byte content matches UTF-8, verify all well-known keys | `tests/test_key_004_metadata_keys.rs` | Gap |
