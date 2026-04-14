//! Key encoding helpers for RocksDB keys (hash, height, epoch, metadata).
//!
//! **Normative**
//! - Height / epoch byte order: [`KEY-001`…`KEY-004`](../docs/requirements/domains/key_encoding/NORMATIVE.md)
//! - [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md) requires this module to exist with key encoding functions.
//!
//! **Rationale:** Big-endian height keys preserve lexicographic sort order matching
//! numeric height order — required for range scans on the canonical index.

/// Encode a chain height as an 8-byte **big-endian** key (canonical index, [`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md)).
#[must_use]
pub fn height_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}
