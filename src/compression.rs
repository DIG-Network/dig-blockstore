//! Zstd compression for block bodies ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md), [`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).
//!
//! Compression logic lives in [`crate::store::BlockStoreInner`]:
//! `serialize_block`, `deserialize_block`, `decompress_block_payload`, `train_dictionary`.
