//! # SNP-005 — Snapshot export/import validation branches (uncovered error paths)
//!
//! **Trace**
//! - Spec: [`SNP-001.md`](../docs/requirements/domains/snapshot/specs/SNP-001.md) (export),
//!   [`SNP-002.md`](../docs/requirements/domains/snapshot/specs/SNP-002.md) (import).
//!
//! ## What this file proves
//!
//! The existing SNP suite covers the happy round-trip, the unsupported-version reject, and
//! checksum/truncation corruption. This file drives the remaining `import_snapshot` /
//! `export_snapshot` validation branches that were otherwise uncovered:
//!
//! - export of a range whose `end_height` has no canonical block → `Serialization("…end_height…")`,
//! - import of a structurally invalid manifest → `Serialization("invalid snapshot manifest…")`,
//! - import where the embedded block height is **non-contiguous** with the manifest range,
//! - import where a block's `parent_hash` breaks the chain link.
//!
//! The contiguity/parent-link cases are built by storing deliberately **mislinked** blocks
//! (`test_block` with a wrong parent) as canonical and exporting them, then importing into a fresh
//! store — fully in-process, no network/mainnet.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_blockstore::{BlockStore, BlockStoreError, SnapshotManifest};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

fn open_store() -> (tempfile::TempDir, BlockStore) {
    let (guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    (guard, store)
}

fn wrong_parent() -> Bytes32 {
    let mut b = [0u8; 32];
    b[0] = 0xAB;
    b[31] = 0xCD;
    Bytes32::new(b)
}

/// **Proves:** SNP-001 export guard — exporting a range whose `end_height` has no canonical block
/// fails fast with a `Serialization` error naming the missing end height (the `get_header_by_height`
/// `None` arm).
#[test]
fn test_export_missing_end_height_errors() {
    let (_g, store) = open_store();
    for b in &build_chain(3) {
        store.extend_chain(b).expect("extend"); // heights 0,1,2
    }
    let mut buf = Vec::new();
    let err = store
        .export_snapshot(0, 99, &mut buf)
        .expect_err("end_height 99 has no canonical block");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("end_height")),
        "expected missing-end-height Serialization error, got {err:?}"
    );
}

/// **Proves:** SNP-002 import guard — a structurally invalid manifest (bytes that are not a valid
/// bincode `SnapshotManifest`) yields the "invalid snapshot manifest" `Serialization` error, covering
/// the manifest deserialize error arm (distinct from the version/checksum arms covered elsewhere).
#[test]
fn test_import_invalid_manifest_errors() {
    let (_g, store) = open_store();
    let mut cursor = std::io::Cursor::new(vec![0xFFu8; 3]);
    let err = store
        .import_snapshot(&mut cursor)
        .expect_err("garbage manifest must error");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("invalid snapshot manifest")),
        "expected invalid-manifest Serialization error, got {err:?}"
    );
}

/// **Proves:** SNP-002 import guard — when the manifest range claims height 0 but the embedded block
/// decodes to height 1, import rejects with "non-contiguous height". We export a genuine single-block
/// snapshot for height 1, then rewrite **only** the leading manifest's range to `[0, 0]` and re-hash
/// the stream so import reaches the contiguity check (not the version or checksum check) first.
#[test]
fn test_import_non_contiguous_height_errors() {
    let (_gsrc, src) = open_store();
    for b in &build_chain(2) {
        src.extend_chain(b).expect("extend"); // heights 0,1
    }
    let mut buf = Vec::new();
    src.export_snapshot(1, 1, &mut buf).expect("export h1");

    // Split the original stream into [manifest | block-entry | 32-byte checksum].
    let orig: SnapshotManifest = bincode::deserialize(&buf[..]).expect("decode manifest");
    let orig_len = bincode::serialized_size(&orig).expect("size") as usize;
    let block_entry = &buf[orig_len..buf.len() - 32];

    // Re-label the range to [0,0]; the block bytes still decode to height 1 → non-contiguous.
    let tampered = SnapshotManifest {
        start_height: 0,
        end_height: 0,
        ..orig
    };
    let manifest_bytes = bincode::serialize(&tampered).expect("serialize tampered");

    // Recompute a valid checksum over the new [manifest || block-entry] so the contiguity check is
    // the failure under test, not a checksum mismatch.
    use chia_sha2::Sha256;
    let mut hasher = Sha256::new();
    hasher.update(&manifest_bytes);
    hasher.update(block_entry);
    let checksum: [u8; 32] = hasher.finalize();

    let mut stitched = Vec::new();
    stitched.extend_from_slice(&manifest_bytes);
    stitched.extend_from_slice(block_entry);
    stitched.extend_from_slice(&checksum);

    let (_gdst, dst) = open_store();
    let mut cursor = std::io::Cursor::new(stitched);
    let err = dst
        .import_snapshot(&mut cursor)
        .expect_err("non-contiguous height must be rejected");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("non-contiguous height")),
        "expected non-contiguous-height Serialization error, got {err:?}"
    );
}

/// **Proves:** SNP-002 import guard — a broken parent link between consecutive blocks is detected.
/// block0 is the genuine genesis; block1 is built with a deliberately wrong `parent_hash` (not
/// block0's hash). Both are stored canonically at heights 0 and 1 and exported as `[0, 1]`; import
/// must reject at height 1 with "broken parent link".
#[test]
fn test_import_broken_parent_link_errors() {
    let (_gsrc, src) = open_store();
    let block0 = test_block(0, Bytes32::default());
    let block1 = test_block(1, wrong_parent()); // parent != block0.hash()
    assert!(src.extend_chain(&block0).expect("extend b0"));
    // extend_chain advances the tip; put block1 canonical at height 1 directly so we don't rely on
    // a valid parent link (extend_chain doesn't validate links, but be explicit about the height).
    assert!(src.put_block(&block1, true).expect("put b1"));

    let mut buf = Vec::new();
    src.export_snapshot(0, 1, &mut buf)
        .expect("export range [0,1]");

    let (_gdst, dst) = open_store();
    let mut cursor = std::io::Cursor::new(buf);
    let err = dst
        .import_snapshot(&mut cursor)
        .expect_err("broken parent link must be rejected");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("broken parent link")),
        "expected broken-parent-link Serialization error, got {err:?}"
    );
}
