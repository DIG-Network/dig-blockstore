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

use crate::constants::CF_BLOCKS;
use crate::encoding::hash_key;
use crate::error::BlockStoreError;
use crate::store::BlockStore;

impl BlockStore {
    /// Export canonical blocks in `[start_height, end_height]` as a snapshot stream.
    ///
    /// # Stream format ([`SNP-001`](../docs/requirements/domains/snapshot/specs/SNP-001.md))
    ///
    /// 1. `SnapshotManifest` (bincode-serialized)
    /// 2. For each height: `block_len: u32 LE` + `compressed_block_bytes`
    /// 3. SHA-256 checksum (32 bytes) of all preceding bytes
    ///
    /// Block bytes are read directly from CF_BLOCKS (pre-compressed) to avoid
    /// decompression/recompression overhead.
    ///
    /// # Returns
    ///
    /// The finalized `SnapshotManifest` with the computed checksum.
    pub fn export_snapshot(
        &self,
        start_height: u64,
        end_height: u64,
        writer: &mut impl std::io::Write,
    ) -> Result<crate::snapshot::SnapshotManifest, BlockStoreError> {
        use crate::snapshot::{SnapshotManifest, SNAPSHOT_VERSION};
        use chia_sha2::Sha256;

        let block_count = end_height.saturating_sub(start_height) + 1;

        // Get state root from header at end_height
        let end_header = self.get_header_by_height(end_height)?.ok_or_else(|| {
            BlockStoreError::Serialization(format!(
                "export_snapshot: no canonical block at end_height {end_height}"
            ))
        })?;

        let mut manifest = SnapshotManifest {
            version: SNAPSHOT_VERSION,
            start_height,
            end_height,
            block_count,
            state_root: end_header.state_root,
            checksum: Bytes32::default(), // placeholder
        };

        let mut hasher = Sha256::new();

        // Write manifest
        let manifest_bytes = bincode::serialize(&manifest)
            .map_err(|e| BlockStoreError::Serialization(e.to_string()))?;
        writer
            .write_all(&manifest_bytes)
            .map_err(|e| BlockStoreError::Serialization(format!("snapshot write: {e}")))?;
        hasher.update(&manifest_bytes);

        // Write blocks: length-prefixed pre-compressed bytes from CF_BLOCKS
        let cf_b = self.cf(CF_BLOCKS)?;
        for height in start_height..=end_height {
            let hash = self.get_hash_by_height(height)?.ok_or_else(|| {
                BlockStoreError::Serialization(format!(
                    "export_snapshot: no canonical hash at height {height}"
                ))
            })?;
            let compressed = self
                .db
                .get_cf(cf_b, hash_key(&hash).as_slice())?
                .ok_or(BlockStoreError::BlockNotFound(hash))?;

            let len = compressed.len() as u32;
            let len_bytes = len.to_le_bytes();
            writer
                .write_all(&len_bytes)
                .map_err(|e| BlockStoreError::Serialization(format!("snapshot write: {e}")))?;
            hasher.update(len_bytes);

            writer
                .write_all(&compressed)
                .map_err(|e| BlockStoreError::Serialization(format!("snapshot write: {e}")))?;
            hasher.update(&compressed);
        }

        // Compute and append SHA-256 checksum
        let checksum_arr: [u8; 32] = hasher.finalize();
        let checksum = Bytes32::new(checksum_arr);
        writer
            .write_all(checksum.as_ref())
            .map_err(|e| BlockStoreError::Serialization(format!("snapshot write: {e}")))?;

        manifest.checksum = checksum;
        Ok(manifest)
    }

    /// Import a snapshot stream, validating manifest, contiguity, parent links, and checksum.
    ///
    /// # Algorithm ([`SNP-002`](../docs/requirements/domains/snapshot/specs/SNP-002.md))
    ///
    /// 1. Read and validate `SnapshotManifest` (schema version check).
    /// 2. For each block: read length-prefixed compressed bytes, decompress + deserialize
    ///    for validation, verify height contiguity and parent-child links, store via `put_block`.
    /// 3. Verify trailing SHA-256 checksum.
    ///
    /// # Returns
    ///
    /// The `SnapshotManifest` read from the stream.
    pub fn import_snapshot(
        &self,
        reader: &mut impl std::io::Read,
    ) -> Result<crate::snapshot::SnapshotManifest, BlockStoreError> {
        use crate::snapshot::SNAPSHOT_VERSION;
        use chia_sha2::Sha256;

        let mut hasher = Sha256::new();

        // Read manifest via bincode (length-aware deserialization)
        let manifest: crate::snapshot::SnapshotManifest = bincode::deserialize_from(&mut *reader)
            .map_err(|e| {
            BlockStoreError::Serialization(format!("invalid snapshot manifest: {e}"))
        })?;
        // Re-serialize to hash the exact wire bytes
        let manifest_bytes = bincode::serialize(&manifest)
            .map_err(|e| BlockStoreError::Serialization(e.to_string()))?;
        hasher.update(&manifest_bytes);

        if manifest.version != SNAPSHOT_VERSION {
            return Err(BlockStoreError::Serialization(format!(
                "unsupported snapshot version: {}",
                manifest.version
            )));
        }

        let mut prev_hash: Option<Bytes32> = None;

        // Read and store blocks
        for expected_height in manifest.start_height..=manifest.end_height {
            // Read u32 LE length prefix
            let mut len_bytes = [0u8; 4];
            reader
                .read_exact(&mut len_bytes)
                .map_err(|e| BlockStoreError::Serialization(format!("snapshot read: {e}")))?;
            hasher.update(len_bytes);
            let block_len = u32::from_le_bytes(len_bytes) as usize;

            // Read compressed block bytes
            let mut compressed = vec![0u8; block_len];
            reader
                .read_exact(&mut compressed)
                .map_err(|e| BlockStoreError::Serialization(format!("snapshot read: {e}")))?;
            hasher.update(&compressed);

            // Decompress and deserialize for validation
            let block = self.deserialize_block(&compressed)?;

            // Validate height contiguity
            if block.height() != expected_height {
                return Err(BlockStoreError::Serialization(format!(
                    "non-contiguous height: expected {expected_height}, got {}",
                    block.height()
                )));
            }

            // Validate parent-child link
            if let Some(prev) = &prev_hash {
                if block.header.parent_hash != *prev {
                    return Err(BlockStoreError::Serialization(format!(
                        "broken parent link at height {expected_height}"
                    )));
                }
            }

            prev_hash = Some(block.hash());

            // Store block as canonical
            self.put_block(&block, true)?;
        }

        // Read and verify trailing checksum
        let mut checksum_bytes = [0u8; 32];
        reader
            .read_exact(&mut checksum_bytes)
            .map_err(|e| BlockStoreError::Serialization(format!("snapshot checksum read: {e}")))?;
        let expected_checksum = Bytes32::new(checksum_bytes);
        let computed_arr: [u8; 32] = hasher.finalize();
        let computed_checksum = Bytes32::new(computed_arr);

        if expected_checksum != computed_checksum {
            return Err(BlockStoreError::Serialization(
                "snapshot checksum mismatch".to_string(),
            ));
        }

        Ok(manifest)
    }
}
