//! # BLK-009 — Attestation storage (`put_attestation` / `get_attestation` on [`CF_ATTESTED`])
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-009.md`](../docs/requirements/domains/block_storage/specs/BLK-009.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-009)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-009-attestation-storage)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Bincode payload under [`CF_ATTESTED`] keyed by block hash | [`test_put_attestation_serializes_to_cf_attested`] |
//! | §2 | `get_attestation` deserializes bincode to [`AttestedBlock`] | [`test_attestation_round_trip_bytes_equal`] |
//! | §3 | Missing key → `Ok(None)` | [`test_get_attestation_unknown_hash_none`] |
//! | §4 | Second put replaces first | [`test_put_attestation_overwrite_latest_wins`] |
//! | Test plan | Raw CF bytes deserialize with bincode independently | [`test_raw_cf_attested_manual_bincode_matches`] |
//! | Parity | Read-only store rejects writes like [`BlockStore::put_block`] | [`test_put_attestation_read_only_rejected`] |
//!
//! **Key encoding:** [`hash_key`] is the same 32-byte raw key as `CF_BLOCKS` / `CF_HEADERS` ([`KEY-001`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md)).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_block::constants::ZERO_HASH;
use dig_block::{AttestedBlock, ReceiptList};
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::hash_key;
use dig_blockstore::{BlockStore, BlockStoreError, CF_ATTESTED};

use common::{temp_blockstore_dir, test_block, test_config};

use chia_protocol::Bytes32;

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Read raw [`CF_ATTESTED`] bytes for `hash` with a **separate** read-only RocksDB handle.
///
/// **Why:** Proves persistence layout without relying on [`BlockStore::get_attestation`] internals
/// ([`BLK-009.md`](../docs/requirements/domains/block_storage/specs/BLK-009.md) test plan “bincode format”).
fn read_cf_attested_raw(path: &Path, hash: &Bytes32) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_ATTESTED)?;
    db.get_cf(cf, hash_key(hash).as_slice()).ok().flatten()
}

/// **Proves:** AC §1 — after [`BlockStore::put_attestation`], a direct `get_cf` on [`CF_ATTESTED`] returns non-empty
/// bytes at [`hash_key`](dig_blockstore::encoding::hash_key)(`hash`).
#[test]
fn test_put_attestation_serializes_to_cf_attested() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let block = test_block(7, ZERO_HASH);
    let hash = block.hash();
    let attested = AttestedBlock::new(block, 16, ReceiptList::default());
    store
        .put_attestation(&hash, &attested)
        .expect("put_attestation");
    drop(store);
    let raw = read_cf_attested_raw(&path, &hash).expect("row exists");
    assert!(
        raw.len() > 32,
        "bincode AttestedBlock payload should be larger than a bare hash key"
    );
}

/// **Proves:** AC §2 — [`BlockStore::get_attestation`] returns bytes that round-trip to the same structure as the
/// original (compared via [`bincode::serialize`] because [`AttestedBlock`] does not implement [`PartialEq`]).
#[test]
fn test_attestation_round_trip_bytes_equal() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(2, ZERO_HASH);
    let hash = block.hash();
    let expected = AttestedBlock::new(block, 8, ReceiptList::default());
    store
        .put_attestation(&hash, &expected)
        .expect("put_attestation");
    let got = store
        .get_attestation(&hash)
        .expect("get_attestation")
        .expect("some");
    assert_eq!(
        bincode::serialize(&got).expect("serialize got"),
        bincode::serialize(&expected).expect("serialize expected"),
        "stored attestation must deserialize to the same bincode bytes as inserted"
    );
}

/// **Proves:** AC §3 — unknown hash returns [`Ok(None)]` without error.
#[test]
fn test_get_attestation_unknown_hash_none() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let unknown = test_block(99, ZERO_HASH).hash();
    assert!(store.get_attestation(&unknown).expect("get").is_none());
}

/// **Proves:** AC §4 — two puts at the same key; [`get_attestation`] reflects the **second** value
/// ([`BLK-009.md`](../docs/requirements/domains/block_storage/specs/BLK-009.md) AC §4).
#[test]
fn test_put_attestation_overwrite_latest_wins() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let block = test_block(5, ZERO_HASH);
    let hash = block.hash();
    let first = AttestedBlock::new(block.clone(), 8, ReceiptList::default());
    let second = AttestedBlock::new(block, 32, ReceiptList::default());
    store.put_attestation(&hash, &first).expect("first");
    store.put_attestation(&hash, &second).expect("second");
    let got = store.get_attestation(&hash).expect("get").expect("some");
    assert_eq!(
        bincode::serialize(&got).unwrap(),
        bincode::serialize(&second).unwrap()
    );
    assert_ne!(
        bincode::serialize(&got).unwrap(),
        bincode::serialize(&first).unwrap(),
        "overwrite must change serialized form (validator set / bitmap size differs)"
    );
}

/// **Proves:** Test plan “bincode format” — raw [`CF_ATTESTED`] value from RocksDB deserializes with standalone
/// [`bincode::deserialize`] to an [`AttestedBlock`] equal-bytes to what [`BlockStore::get_attestation`] returns.
#[test]
fn test_raw_cf_attested_manual_bincode_matches() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let block = test_block(11, ZERO_HASH);
    let hash = block.hash();
    let attested = AttestedBlock::new(block, 10, ReceiptList::default());
    store
        .put_attestation(&hash, &attested)
        .expect("put_attestation");
    drop(store);
    let raw = read_cf_attested_raw(&path, &hash).expect("raw row");
    let manual: AttestedBlock = bincode::deserialize(&raw).expect("manual bincode");
    let via_api = {
        let s = BlockStore::open(test_config(path)).expect("reopen");
        s.get_attestation(&hash).expect("get").expect("some")
    };
    assert_eq!(
        bincode::serialize(&manual).unwrap(),
        bincode::serialize(&via_api).unwrap()
    );
}

/// **Proves:** Read-only stores reject attestation writes with the same [`ERR_MUTATION_READ_ONLY`] surface as block puts.
#[test]
fn test_put_attestation_read_only_rejected() {
    let (_guard, path) = temp_blockstore_dir();
    let path_ro = path.clone();
    {
        let s = BlockStore::open(test_config(path)).expect("open rw");
        let b = test_block(0, ZERO_HASH);
        s.put_block(&b, false).expect("seed block");
    }
    let ro = BlockStore::open_readonly(path_ro).expect("ro");
    let block = test_block(1, ZERO_HASH);
    let attested = AttestedBlock::new(block.clone(), 4, ReceiptList::default());
    let err = ro
        .put_attestation(&block.hash(), &attested)
        .expect_err("read-only must error");
    match err {
        BlockStoreError::Serialization(s) => {
            assert!(s.contains("read-only") || s.contains("read only"), "{s}");
        }
        other => panic!("unexpected {other:?}"),
    }
}
