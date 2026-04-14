# Snapshot - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| SNP-001 | gap | Export canonical block range as snapshot with manifest, length-prefixed blocks, and SHA-256 checksum | Integration: export a known block range, parse output to verify manifest header, block framing (u32 LE length + compressed bytes), and trailing checksum; verify blocks are pre-compressed from CF_BLOCKS (no recompression) |
| SNP-002 | gap | Import snapshot with manifest validation, contiguity checks, parent-child link validation, and checksum verification | Integration: import a valid snapshot and verify all blocks stored; import with non-sequential heights and verify rejection; import with broken parent-child link and verify rejection; import with corrupted checksum and verify rejection |
| SNP-003 | gap | SnapshotManifest struct with version, start/end height, block_count, state_root, checksum; derives Serialize/Deserialize | Unit: construct SnapshotManifest, bincode round-trip, verify all fields preserved; verify Serialize and Deserialize trait implementations |
| SNP-004 | gap | SHA-256 checksum computed over all bytes before checksum on export; verified incrementally on import; reject on mismatch | Integration: export snapshot and verify checksum matches manual SHA-256 of preceding bytes; corrupt a single byte in snapshot and verify import rejects with BlockStoreError::Serialization |
