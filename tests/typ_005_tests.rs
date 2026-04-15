//! # TYP-005 — [`dig_blockstore::StoredCheckpoint`] and `CF_CHECKPOINTS` wire shape
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-005.md`](../docs/requirements/domains/storage_types/specs/TYP-005.md)
//! - NORMATIVE: [`NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md#typ-005-storedcheckpoint-struct)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Field coverage:** Construct [`StoredCheckpoint`] with representative [`dig_block::Checkpoint`],
//!   [`dig_block::SignerBitmap`], and [`chia_bls`] keys/signatures, then assert each field round-trips
//!   through [`bincode`] per [`TYP-005`](../docs/requirements/domains/storage_types/specs/TYP-005.md)
//!   (`CF_CHECKPOINTS` value encoding).
//! - **Optionals:** Separate tests for `l1_height` / `l1_coin_id` being `None` vs `Some` prove serde
//!   handles both shapes CKP rows will use before and after L1 confirmation.
//! - **RocksDB row:** One integration test opens a throwaway DB with the same CF descriptors as
//!   [`dig_blockstore::cf_options::column_family_descriptors`], writes under [`dig_blockstore::CF_CHECKPOINTS`]
//!   with [`dig_blockstore::epoch_key`], and reads back — satisfying “stored and retrieved” without
//!   requiring [`dig_blockstore::BlockStore`] to expose checkpoint APIs yet ([`CKP-001`](../docs/requirements/domains/checkpoint_storage/specs/CKP-001_put_checkpoint.md) precursor).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_bls::{PublicKey, Signature};
use chia_protocol::Bytes32;
use dig_block::{Checkpoint, SignerBitmap};
use dig_blockstore::{cf_options, epoch_key, BlockStoreConfig, StoredCheckpoint, CF_CHECKPOINTS};
use rocksdb::{Options, DB};

fn sample_signer_bitmap() -> SignerBitmap {
    let mut b = SignerBitmap::new(8);
    b.set_signed(0).expect("index in range");
    b.set_signed(3).expect("index in range");
    b
}

/// Non-trivial [`Checkpoint`] (dig-block CKP-001 shape) so bincode round-trips exercise real fields.
fn sample_checkpoint() -> Checkpoint {
    Checkpoint {
        epoch: 77,
        state_root: Bytes32::new([2u8; 32]),
        block_root: Bytes32::new([3u8; 32]),
        block_count: 100,
        tx_count: 4_000,
        total_fees: 99_000,
        prev_checkpoint: Bytes32::new([4u8; 32]),
        withdrawals_root: Bytes32::new([5u8; 32]),
        withdrawal_count: 6,
    }
}

fn sample_stored(
    l1_height: Option<u32>,
    l1_coin_id: Option<Bytes32>,
    stored_at: u64,
) -> StoredCheckpoint {
    StoredCheckpoint {
        checkpoint: sample_checkpoint(),
        signer_bitmap: sample_signer_bitmap(),
        aggregate_signature: Signature::default(),
        aggregate_pubkey: PublicKey::default(),
        score: 1_234,
        submitter: 7,
        l1_height,
        l1_coin_id,
        stored_at,
    }
}

#[test]
fn test_stored_checkpoint_all_fields() {
    let coin = Bytes32::new([0xAB; 32]);
    let sc = sample_stored(Some(99), Some(coin), 1_700_000_001);
    assert_eq!(sc.checkpoint, sample_checkpoint());
    assert_eq!(sc.signer_bitmap.signer_count(), 2);
    assert_eq!(sc.score, 1_234);
    assert_eq!(sc.submitter, 7);
    assert_eq!(sc.l1_height, Some(99));
    assert_eq!(sc.l1_coin_id, Some(coin));
    assert_eq!(sc.stored_at, 1_700_000_001);
}

#[test]
fn test_stored_checkpoint_bincode_roundtrip() {
    let sc = sample_stored(None, None, 42);
    let bytes = sc.encode_bincode().expect("serialize");
    let back = StoredCheckpoint::decode_bincode(&bytes).expect("deserialize");
    assert_eq!(sc, back);
}

#[test]
fn test_stored_checkpoint_optional_none_roundtrip() {
    let sc = sample_stored(None, None, 100);
    let back = StoredCheckpoint::decode_bincode(&sc.encode_bincode().unwrap()).unwrap();
    assert_eq!(back.l1_height, None);
    assert_eq!(back.l1_coin_id, None);
}

#[test]
fn test_stored_checkpoint_optional_some_roundtrip() {
    let coin = Bytes32::new([1u8; 32]);
    let sc = sample_stored(Some(12_345), Some(coin), 200);
    let back = StoredCheckpoint::decode_bincode(&sc.encode_bincode().unwrap()).unwrap();
    assert_eq!(back.l1_height, Some(12_345));
    assert_eq!(back.l1_coin_id, Some(coin));
}

#[test]
fn test_stored_checkpoint_clone() {
    let sc = sample_stored(None, None, 0);
    assert_eq!(sc.clone(), sc);
}

#[test]
fn test_stored_checkpoint_debug() {
    let sc = sample_stored(None, None, 0);
    let s = format!("{sc:?}");
    assert!(s.contains("StoredCheckpoint"));
}

#[test]
fn test_stored_checkpoint_cf_checkpoints_roundtrip() {
    let (_dir, path) = common::temp_blockstore_dir();
    let cfg = BlockStoreConfig {
        path: path.clone(),
        ..common::test_config(path.clone())
    };

    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cfs = cf_options::column_family_descriptors(&cfg, None);
    let db = DB::open_cf_descriptors(&opts, &path, cfs).expect("open_cf_descriptors");

    let cf = db
        .cf_handle(CF_CHECKPOINTS)
        .expect("CF_CHECKPOINTS must exist after open");

    let epoch = 42u64;
    let sc = sample_stored(Some(5000), Some(Bytes32::new([0xCD; 32])), 1_700_000_042);
    let key = epoch_key(epoch);
    let bytes = sc.encode_bincode().unwrap();
    db.put_cf(cf, key.as_slice(), &bytes).expect("put_cf");

    let read = db
        .get_cf(cf, key.as_slice())
        .expect("get_cf")
        .expect("value present");
    let got = StoredCheckpoint::decode_bincode(&read).expect("decode");
    assert_eq!(got, sc);

    drop(db);
}
