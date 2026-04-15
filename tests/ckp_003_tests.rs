//! # CKP-003 — `get_latest_checkpoint`: reverse iterator for highest epoch
//!
//! **Trace**
//! - Spec: [`CKP-003_get_latest_checkpoint.md`](../docs/requirements/domains/checkpoint_storage/specs/CKP-003_get_latest_checkpoint.md)

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
fn test_get_latest_checkpoint_none() {
    // **Proves:** CKP-003 AC §3 — no checkpoints returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert!(store.get_latest_checkpoint().expect("get").is_none());
}

#[test]
fn test_get_latest_checkpoint_returns_highest_epoch() {
    // **Proves:** CKP-003 AC §2 — returns checkpoint with highest epoch.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    for epoch in [5, 10, 3, 20, 15] {
        store
            .put_checkpoint(&sample_checkpoint(epoch))
            .expect("put");
    }
    let latest = store.get_latest_checkpoint().expect("get").expect("some");
    assert_eq!(latest.checkpoint.epoch, 20, "highest epoch is 20");
}

#[test]
fn test_get_latest_checkpoint_updates_after_new_insert() {
    // **Proves:** CKP-003 AC §4 — adding a higher epoch updates the latest.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(10)).expect("put");
    assert_eq!(
        store
            .get_latest_checkpoint()
            .expect("g")
            .expect("s")
            .checkpoint
            .epoch,
        10
    );
    store.put_checkpoint(&sample_checkpoint(50)).expect("put");
    assert_eq!(
        store
            .get_latest_checkpoint()
            .expect("g")
            .expect("s")
            .checkpoint
            .epoch,
        50
    );
}
