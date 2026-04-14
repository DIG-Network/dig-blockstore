# Key Encoding - Verification Matrix

| Req ID | Requirement | Verification Method | Test File(s) | Status |
|--------|-------------|-------------------|--------------|--------|
| KEY-001 | Hash keys MUST be raw 32-byte Bytes32 values with no prefix | Unit test: encode hash key and assert length == 32, content matches raw hash bytes | `tests/unit/key_encoding/test_key_001_hash_keys.rs` | Gap |
| KEY-002 | Height keys MUST be u64 big-endian 8-byte encoding with correct sort order | Unit test: encode height keys, assert length == 8, assert lexicographic order matches numeric order | `tests/unit/key_encoding/test_key_002_height_keys.rs` | Gap |
| KEY-003 | Epoch keys MUST be u64 big-endian 8-byte encoding | Unit test: encode epoch keys, assert length == 8, assert lexicographic order matches numeric order | `tests/unit/key_encoding/test_key_003_epoch_keys.rs` | Gap |
| KEY-004 | Metadata keys MUST be UTF-8 encoded strings | Unit test: encode metadata keys, assert byte content matches UTF-8, verify all well-known keys | `tests/unit/key_encoding/test_key_004_metadata_keys.rs` | Gap |
