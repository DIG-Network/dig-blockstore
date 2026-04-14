//! Zstd compression for block bodies ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md), [`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).
//!
//! **Stack:** [`zstd`](https://docs.rs/zstd) with optional dictionary from [`crate::constants::META_ZSTD_DICT`].

/// Owns compressor state (dictionary id, level) once block ingestion exists.
#[derive(Debug, Default)]
pub struct CompressionPipeline {}
