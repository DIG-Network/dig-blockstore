//! Snapshot export / import ([`SNP-001`…`SNP-004`](../docs/requirements/domains/snapshot/NORMATIVE.md)).
//!
//! # Overview
//!
//! Snapshots enable fast bootstrapping: a new node downloads a snapshot file containing
//! a contiguous range of canonical blocks, verifies the SHA-256 checksum, and bulk-inserts
//! into its local store — skipping full block-by-block sync.
//!
//! # Stream format
//!
//! ```text
//! [ SnapshotManifest (bincode) ]
//! [ block_0_len: u32 LE ] [ block_0_compressed_bytes ]
//! [ block_1_len: u32 LE ] [ block_1_compressed_bytes ]
//! ...
//! [ block_N_len: u32 LE ] [ block_N_compressed_bytes ]
//! [ SHA-256 checksum: 32 bytes ]
//! ```
//!
//! The checksum covers all bytes from the start of the manifest to the end of the last block
//! (i.e., everything except the trailing 32-byte checksum itself).
//!
//! # Requirements
//!
//! - [`SNP-001`](../docs/requirements/domains/snapshot/specs/SNP-001.md) — export
//! - [`SNP-002`](../docs/requirements/domains/snapshot/specs/SNP-002.md) — import
//! - [`SNP-003`](../docs/requirements/domains/snapshot/specs/SNP-003.md) — SnapshotManifest struct
//! - [`SNP-004`](../docs/requirements/domains/snapshot/specs/SNP-004.md) — checksum verification

use chia_protocol::Bytes32;
use serde::{Deserialize, Serialize};

/// Metadata header for snapshot files ([`SNP-003`](../docs/requirements/domains/snapshot/specs/SNP-003.md)).
///
/// Written as the first bytes of every snapshot stream (bincode-encoded).
/// The importer reads this to validate the schema version and expected block range
/// before processing block data.
///
/// # Fields
///
/// - `version` — schema version for forward compatibility (current: 1).
/// - `start_height` / `end_height` — inclusive block range.
/// - `block_count` — must equal `end_height - start_height + 1`.
/// - `state_root` — state root hash at `end_height` for post-import verification.
/// - `checksum` — SHA-256 of all block data (placeholder during export, filled after).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    /// Schema version (current: 1). Importers reject unknown versions.
    pub version: u32,
    /// First block height included (inclusive).
    pub start_height: u64,
    /// Last block height included (inclusive).
    pub end_height: u64,
    /// Total blocks. Must equal `end_height - start_height + 1`.
    pub block_count: u64,
    /// State root hash at `end_height`.
    pub state_root: Bytes32,
    /// SHA-256 checksum of all block data bytes (after manifest, before checksum).
    pub checksum: Bytes32,
}

/// Current snapshot schema version.
pub const SNAPSHOT_VERSION: u32 = 1;

/// Facade for snapshot file I/O.
#[derive(Debug, Default)]
pub struct SnapshotIo {}
