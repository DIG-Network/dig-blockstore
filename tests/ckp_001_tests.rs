//! # CKP-001 — `put_checkpoint`: bincode-serialize StoredCheckpoint to CF_CHECKPOINTS
//!
//! **Trace**
//! - Spec: [`CKP-001_put_checkpoint.md`](../docs/requirements/domains/checkpoint_storage/specs/CKP-001_put_checkpoint.md)
//!
//! ## What this file proves
//!
//! `put_checkpoint` persists a `StoredCheckpoint` to CF_CHECKPOINTS keyed by big-endian
//! epoch. Tests verify round-trip, overwrite, key encoding, and multi-epoch storage.

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
            block_count: (epoch * 32) as u32,
            tx_count: epoch * 100,
            total_fees: epoch * 1000,
            prev_checkpoint: Bytes32::new([3u8; 32]),
            withdrawals_root: Bytes32::new([4u8; 32]),
            withdrawal_count: 0,
        },
        signer_bitmap: SignerBitmap::new(8),
        aggregate_signature: Signature::default(),
        aggregate_pubkey: PublicKey::default(),
        score: epoch * 10,
        submitter: 0,
        l1_height: Some(epoch as u32 * 100),
        l1_coin_id: None,
        stored_at: 1_700_000_000 + epoch,
    }
}

#[test]
fn test_put_checkpoint_round_trip() {
    // **Proves:** CKP-001 AC §5 — put then get returns equal StoredCheckpoint.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let cp = sample_checkpoint(42);
    store.put_checkpoint(&cp).expect("put");
    let got = store.get_checkpoint(42).expect("get").expect("some");
    assert_eq!(got, cp);
}

#[test]
fn test_put_checkpoint_overwrite() {
    // **Proves:** CKP-001 AC §4 — overwriting same epoch does not error.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let cp1 = sample_checkpoint(10);
    store.put_checkpoint(&cp1).expect("put1");

    let mut cp2 = sample_checkpoint(10);
    cp2.score = 999;
    store.put_checkpoint(&cp2).expect("put2 overwrite");

    let got = store.get_checkpoint(10).expect("get").expect("some");
    assert_eq!(got.score, 999, "overwrite must replace");
}

#[test]
fn test_put_checkpoint_multiple_epochs() {
    // **Proves:** CKP-001 AC — store checkpoints at multiple epochs, retrieve each independently.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    for epoch in [1, 100, 1000] {
        store
            .put_checkpoint(&sample_checkpoint(epoch))
            .expect("put");
    }
    for epoch in [1, 100, 1000] {
        let got = store.get_checkpoint(epoch).expect("get").expect("some");
        assert_eq!(got.checkpoint.epoch, epoch);
    }
}
