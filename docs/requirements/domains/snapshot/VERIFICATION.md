# Snapshot - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| SNP-001 | done | Export canonical block range as snapshot with manifest, length-prefixed blocks, and SHA-256 checksum | `tests/snp_001_tests.rs` — basic export, export+import round-trip, partial range. 3 tests. |
| SNP-002 | done | Import snapshot with manifest validation, contiguity checks, parent-child link validation, and checksum verification | `tests/snp_002_tests.rs` — checksum validation, bad version rejection, blocks stored canonically. 3 tests. |
| SNP-003 | done | SnapshotManifest struct with version, start/end height, block_count, state_root, checksum; derives Serialize/Deserialize | `tests/snp_003_tests.rs` — bincode round-trip, all fields preserved, version constant, zero values. 4 tests. |
| SNP-004 | done | SHA-256 checksum computed over all bytes before checksum on export; verified incrementally on import; reject on mismatch | `tests/snp_004_tests.rs` — valid passes, single-byte corruption detected, truncated detected, deterministic. 4 tests. |
