//! # CKP-004 — `get_checkpoints_in_range`: forward iterator over epoch range
//!
//! **Trace**
//! - Spec: [`CKP-004_get_checkpoints_in_range.md`](../docs/requirements/domains/checkpoint_storage/specs/CKP-004_get_checkpoints_in_range.md)

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_bls::{PublicKey, Signature};
use chia_protocol::Bytes32;
use dig_block::{Checkpoint, SignerBitmap};
use dig_blockstore::{BlockStore, StoredCheckpoint};

use common::{temp_blockstore_dir, test_config};

fn sample_checkpoint(epoch: u64) -> StoredCheckpoint {
    StoredCheckpoint {
        checkpoint: Checkpoint {
            epoch,
            state_root: Bytes32::default(),
            block_root: Bytes32::default(),
            block_count: 0,
            tx_count: 0,
            total_fees: 0,
            prev_checkpoint: Bytes32::default(),
            withdrawals_root: Bytes32::default(),
            withdrawal_count: 0,
        },
        signer_bitmap: SignerBitmap::new(4),
        aggregate_signature: Signature::default(),
        aggregate_pubkey: PublicKey::default(),
        score: epoch,
        submitter: 0,
        l1_height: None,
        l1_coin_id: None,
        stored_at: 0,
    }
}

#[test]
fn test_checkpoints_in_range_basic() {
    // **Proves:** CKP-004 AC — returns checkpoints within inclusive range.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    for epoch in [5, 10, 15, 20, 25] {
        store
            .put_checkpoint(&sample_checkpoint(epoch))
            .expect("put");
    }

    let range = store.get_checkpoints_in_range(10, 20).expect("range");
    let epochs: Vec<u64> = range.iter().map(|c| c.checkpoint.epoch).collect();
    assert_eq!(epochs, vec![10, 15, 20], "inclusive range [10,20]");
}

#[test]
fn test_checkpoints_in_range_empty() {
    // **Proves:** CKP-004 AC — empty range returns empty Vec.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(5)).expect("put");

    let range = store.get_checkpoints_in_range(100, 200).expect("range");
    assert!(range.is_empty(), "no checkpoints in [100,200]");
}

#[test]
fn test_checkpoints_in_range_inverted() {
    // **Proves:** CKP-004 AC — start > end returns empty Vec, not error.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(10)).expect("put");

    let range = store.get_checkpoints_in_range(20, 5).expect("range");
    assert!(range.is_empty());
}

#[test]
fn test_checkpoints_in_range_single() {
    // **Proves:** CKP-004 AC — start == end returns single checkpoint if it exists.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(42)).expect("put");

    let range = store.get_checkpoints_in_range(42, 42).expect("range");
    assert_eq!(range.len(), 1);
    assert_eq!(range[0].checkpoint.epoch, 42);
}

#[test]
fn test_checkpoints_in_range_gaps() {
    // **Proves:** CKP-004 — gaps in epoch numbers are handled; only stored epochs returned.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    for epoch in [1, 5, 10] {
        store
            .put_checkpoint(&sample_checkpoint(epoch))
            .expect("put");
    }

    let range = store.get_checkpoints_in_range(0, 100).expect("range");
    let epochs: Vec<u64> = range.iter().map(|c| c.checkpoint.epoch).collect();
    assert_eq!(epochs, vec![1, 5, 10]);
}
