# Snapshot - Normative Requirements

- **Domain:** snapshot
- **Prefix:** SNP
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### SNP-001: Export Snapshot

`export_snapshot(&self, start_height: u64, end_height: u64, writer: &mut impl Write) -> Result<SnapshotManifest>`

1. MUST write the `SnapshotManifest` (bincode-serialized) as the first bytes of the stream.
2. MUST then write each canonical block in the range `[start_height, end_height]` as: `block_len` (u32 little-endian) followed by the compressed block bytes (pre-compressed from `CF_BLOCKS` to avoid recompression).
3. MUST append a SHA-256 checksum (32 bytes) of all preceding bytes written.

**Spec reference:** 14.1

---

### SNP-002: Import Snapshot

`import_snapshot(&self, reader: &mut impl Read) -> Result<SnapshotManifest>`

1. MUST read and validate the `SnapshotManifest` (schema version check).
2. MUST read blocks sequentially from the stream.
3. MUST validate contiguity: block heights MUST be sequential.
4. MUST validate parent-child links: `block.parent_hash` MUST equal the previous block's hash.
5. MUST store imported blocks via the write pipeline for batched ingestion.
6. MUST verify the trailing SHA-256 checksum matches the incrementally computed hash of all preceding bytes.

**Spec reference:** 14.2

---

### SNP-003: SnapshotManifest Struct

`SnapshotManifest` MUST contain the following fields:

- `schema_version: u32` -- schema version for forward compatibility.
- `start_height: u64` -- first block height in the snapshot.
- `end_height: u64` -- last block height in the snapshot.
- `block_count: u64` -- total number of blocks in the snapshot.
- `start_hash: Bytes32` -- hash of the first block in the snapshot.
- `end_hash: Bytes32` -- hash of the last block in the snapshot (new tip after import).
- `total_bytes: u64` -- total snapshot size in bytes.
- `compressed: bool` -- whether blocks are pre-compressed.
- `checksum: Bytes32` -- SHA-256 of all block data (chia-sha2).

MUST derive `Serialize` and `Deserialize`.

**Spec reference:** 14.3

---

### SNP-004: Checksum Verification

Snapshot export MUST compute SHA-256 (via `chia-sha2::Sha256`) over all bytes written before the checksum itself. Import MUST incrementally compute SHA-256 over all bytes read, then verify against the trailing 32-byte checksum. MUST reject with `BlockStoreError::Serialization` on mismatch.

**Spec reference:** 14.1, 14.2
