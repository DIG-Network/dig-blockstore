# Serialization - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| SER-001 | done | Block serialization with bincode + zstd dictionary compression; read path reverses | Unit: `tests/ser_001_tests.rs` — round-trip, zstd magic, 2–6× ratio, dictionary frame ID bit, plain fallback, dict→plain fallback, corrupt→Serialization, level 3 default |
| SER-002 | gap | Header serialization with bincode only, no compression | Unit: round-trip serialize/deserialize header; verify raw bytes in CF_HEADERS are valid bincode (no zstd framing); verify deserialization without decompression succeeds |
| SER-003 | gap | Wire-format conversion via chia-traits Streamable for peer gossip | Unit: round-trip block_to_wire_bytes/block_from_wire_bytes; verify wire bytes differ from storage bytes; verify Streamable compatibility with chia-traits |
| SER-004 | gap | Round-trip identity for bincode, zstd, dictionary compression, and block hashing | Unit: bincode round-trip identity for L2Block, L2BlockHeader, BlockRecord; zstd round-trip identity; dictionary-compressed round-trip identity; hash invariance across cycles |
| SER-005 | gap | Dictionary training after 1000 blocks, persistence to CF_METADATA, fallback to plain zstd | Integration: verify plain-zstd mode on fresh startup; store 1000+ blocks and verify dictionary trained; verify dictionary persisted to META_ZSTD_DICT; verify dictionary loaded on restart; verify fallback decompression for pre-dictionary blocks |
