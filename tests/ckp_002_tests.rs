//! # CKP-002 — `get_checkpoint`: retrieve by epoch from CF_CHECKPOINTS
//!
//! **Trace**
//! - Spec: [`CKP-002_get_checkpoint.md`](../docs/requirements/domains/checkpoint_storage/specs/CKP-002_get_checkpoint.md)

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
            state_root: Bytes32::new([1u8; 32]),
            block_root: Bytes32::new([2u8; 32]),
            block_count: 32,
            tx_count: 100,
            total_fees: 1000,
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
fn test_get_checkpoint_missing() {
    // **Proves:** CKP-002 AC §3 — non-existent epoch returns None.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    assert!(store.get_checkpoint(999).expect("get").is_none());
}

#[test]
fn test_get_checkpoint_existing() {
    // **Proves:** CKP-002 AC §4 — stored epoch returns full checkpoint.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let cp = sample_checkpoint(7);
    store.put_checkpoint(&cp).expect("put");
    let got = store.get_checkpoint(7).expect("get").expect("some");
    assert_eq!(got, cp);
}

#[test]
fn test_get_checkpoint_no_cross_contamination() {
    // **Proves:** CKP-002 AC — epochs are independent; getting one does not affect others.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    store.put_checkpoint(&sample_checkpoint(10)).expect("p10");
    store.put_checkpoint(&sample_checkpoint(20)).expect("p20");

    let c10 = store.get_checkpoint(10).expect("g10").expect("some");
    let c20 = store.get_checkpoint(20).expect("g20").expect("some");
    assert_eq!(c10.checkpoint.epoch, 10);
    assert_eq!(c20.checkpoint.epoch, 20);
    assert!(store.get_checkpoint(15).expect("g15").is_none());
}
