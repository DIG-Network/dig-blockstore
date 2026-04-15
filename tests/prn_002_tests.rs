//! # PRN-002 — `prune_checkpoints_before_epoch`: remove checkpoints below epoch
//!
//! **Trace**
//! - Spec: [`PRN-002_prune_checkpoints_before_epoch.md`](../docs/requirements/domains/pruning/specs/PRN-002_prune_checkpoints_before_epoch.md)

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
        score: 0,
        submitter: 0,
        l1_height: None,
        l1_coin_id: None,
        stored_at: 0,
    }
}

#[test]
fn test_prune_checkpoints_basic() {
    // **Proves:** PRN-002 AC §1/§3 — deletes checkpoints below target epoch.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    for epoch in [5, 10, 15, 20, 25] {
        store
            .put_checkpoint(&sample_checkpoint(epoch))
            .expect("put");
    }

    let count = store.prune_checkpoints_before_epoch(15).expect("prune");
    assert_eq!(count, 2, "epochs 5 and 10 pruned");

    // Pruned
    assert!(store.get_checkpoint(5).expect("g").is_none());
    assert!(store.get_checkpoint(10).expect("g").is_none());
    // Retained
    assert!(store.get_checkpoint(15).expect("g").is_some());
    assert!(store.get_checkpoint(20).expect("g").is_some());
    assert!(store.get_checkpoint(25).expect("g").is_some());
}

#[test]
fn test_prune_checkpoints_zero_epoch() {
    // **Proves:** PRN-002 AC §6 — epoch 0 returns 0.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(5)).expect("put");
    assert_eq!(store.prune_checkpoints_before_epoch(0).expect("prune"), 0);
}

#[test]
fn test_prune_checkpoints_none_below() {
    // **Proves:** PRN-002 AC §5 — no checkpoints below target returns 0.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(100)).expect("put");
    assert_eq!(store.prune_checkpoints_before_epoch(50).expect("prune"), 0);
}
