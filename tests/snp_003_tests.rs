//! # SNP-003 — `SnapshotManifest` struct with bincode serde
//!
//! **Trace**
//! - Spec: [`SNP-003.md`](../docs/requirements/domains/snapshot/specs/SNP-003.md)

#![forbid(unsafe_code)]

use chia_protocol::Bytes32;
use dig_blockstore::{SnapshotManifest, SNAPSHOT_VERSION};

#[test]
fn test_snapshot_manifest_bincode_round_trip() {
    let m = SnapshotManifest {
        version: SNAPSHOT_VERSION,
        start_height: 100,
        end_height: 199,
        block_count: 100,
        state_root: Bytes32::new([0xAA; 32]),
        checksum: Bytes32::new([0xBB; 32]),
    };
    let bytes = bincode::serialize(&m).expect("serialize");
    let back: SnapshotManifest = bincode::deserialize(&bytes).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn test_snapshot_manifest_all_fields_preserved() {
    let m = SnapshotManifest {
        version: 42,
        start_height: 1000,
        end_height: 2000,
        block_count: 1001,
        state_root: Bytes32::new([1u8; 32]),
        checksum: Bytes32::new([2u8; 32]),
    };
    let bytes = bincode::serialize(&m).expect("serialize");
    let back: SnapshotManifest = bincode::deserialize(&bytes).expect("deserialize");
    assert_eq!(back.version, 42);
    assert_eq!(back.start_height, 1000);
    assert_eq!(back.end_height, 2000);
    assert_eq!(back.block_count, 1001);
    assert_eq!(back.state_root, Bytes32::new([1u8; 32]));
    assert_eq!(back.checksum, Bytes32::new([2u8; 32]));
}

#[test]
fn test_snapshot_manifest_version_constant() {
    assert_eq!(SNAPSHOT_VERSION, 1);
}

#[test]
fn test_snapshot_manifest_zero_values() {
    let m = SnapshotManifest {
        version: 0,
        start_height: 0,
        end_height: 0,
        block_count: 0,
        state_root: Bytes32::default(),
        checksum: Bytes32::default(),
    };
    let bytes = bincode::serialize(&m).expect("serialize");
    let back: SnapshotManifest = bincode::deserialize(&bytes).expect("deserialize");
    assert_eq!(m, back);
}
