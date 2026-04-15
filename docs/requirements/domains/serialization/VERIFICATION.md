# Serialization - Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| SER-001 | done | Block serialization with bincode + zstd dictionary compression; read path reverses | Unit: `tests/ser_001_tests.rs` — round-trip, zstd magic, 2–6× ratio, dictionary frame ID bit, plain fallback, dict→plain fallback, corrupt→Serialization, level 3 default |
| SER-002 | done | Header serialization with bincode only, no compression | Unit: `tests/ser_002_tests.rs` — round-trip PartialEq, no zstd magic, direct `bincode::deserialize`, bounded size, truncate/empty → Serialization, `init_genesis` CF_HEADERS bytes match `serialize_header`, distinct headers → distinct bytes |
| SER-003 | done | Wire-format conversion via chia-traits Streamable for peer gossip | Unit: `tests/ser_003_tests.rs` — hash round-trip, wire ≠ bincode, `L2Block::parse` vs façade, invalid/truncated/trailing → Serialization, no zstd magic, deterministic bytes, BlockStore smoke |
| SER-004 | done | Round-trip identity for bincode, zstd, dictionary compression, and block hashing | Unit: `tests/ser_004_tests.rs` — bincode `L2BlockHeader`/stable-bytes+hash for `L2Block`+`AttestedBlock`, `BlockRecord` clone (no Serialize per TYP-004), plain/dict zstd bytes, `BlockStore` hash invariance; `SnapshotManifest` deferred |
| SER-005 | done | Dictionary training after 1000 blocks, persistence to CF_METADATA, fallback to plain zstd | Integration: `tests/ser_005_tests.rs` — plain-zstd before metadata dict; threshold+`META_ZSTD_DICT`; reopen loads dict; genesis readable after train; size band; no double-train; mixed-mode reads; read-only `put` guard |
