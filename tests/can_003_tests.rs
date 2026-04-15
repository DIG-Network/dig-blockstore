//! # CAN-003 — `set_canonical` (promote stored block to canonical index)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`CAN-003.md`](../docs/requirements/domains/canonical_chain/specs/CAN-003.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-003)](../docs/requirements/domains/canonical_chain/NORMATIVE.md#can-003-set_canonical)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! [`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md) requires [`BlockStore::set_canonical`](dig_blockstore::BlockStore::set_canonical) to:
//! - reject unknown hashes with [`BlockStoreError::BlockNotInStore`](dig_blockstore::BlockStoreError::BlockNotInStore),
//! - write [`CF_CANONICAL`](dig_blockstore::CF_CANONICAL) and the `canonical.bin` mmap sidecar at `height × 32`,
//! - set [`BlockRecord::in_canonical_chain`](dig_blockstore::BlockRecord::in_canonical_chain) in the record cache,
//! - be idempotent on repeat calls, and
//! - allow a **later** hash at the **same** height to overwrite the height index (reorg / fork competition staging).
//!
//! **Fixture note:** [`dig_block::BlockStatus::Validated`](dig_block::BlockStatus::Validated) implies
//! [`BlockStatus::is_canonical`](dig_block::BlockStatus::is_canonical) is already `true`, so the “record flips to
//! canonical” assertion uses [`BlockStore::update_status`](dig_blockstore::BlockStore::update_status) with
//! [`BlockStatus::Orphaned`](dig_block::BlockStatus::Orphaned) first to force `in_canonical_chain == false`, then
//! calls `set_canonical` — matching the CAN-003 acceptance row without contradicting [`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md) status semantics.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::BlockStatus;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::height_key;
use dig_blockstore::{BlockStore, BlockStoreError, CF_CANONICAL};

use common::{temp_blockstore_dir, test_block, test_config};

fn canonical_bin_path(db: &Path) -> std::path::PathBuf {
    db.join("canonical.bin")
}

fn open_opts_write() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Read the 32-byte canonical hash at `height` from a **separate** `DB` handle (release [`BlockStore`] first on Windows).
fn cf_canonical_hash_at(db_path: &Path, height: u64) -> Option<[u8; 32]> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db =
        rocksdb::DB::open_cf_descriptors(&open_opts_write(), db_path, cfs).expect("open for probe");
    let cf = db.cf_handle(CF_CANONICAL).expect("cf");
    let v = db.get_cf(cf, height_key(height).as_slice()).expect("get")?;
    let s: &[u8] = v.as_ref();
    <[u8; 32]>::try_from(s).ok()
}

/// **Proves:** CAN-003 test plan `test_set_canonical_success` + AC “CF_CANONICAL + mmap updated after success”.
#[test]
fn test_set_canonical_success() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b = test_block(3, Bytes32::default());
    let h = b.hash();
    assert!(store.put_block(&b, false).expect("put"));
    store.set_canonical(&h).expect("set_canonical");
    drop(store);

    let got = cf_canonical_hash_at(&path, 3).expect("cf row");
    let want: [u8; 32] = h.as_ref().try_into().expect("hash bytes");
    assert_eq!(got, want);
    let bin = std::fs::read(canonical_bin_path(&path)).expect("bin");
    let sl: &[u8] = &bin[(3usize * 32)..(3usize * 32 + 32)];
    let mmap_h: [u8; 32] = <[u8; 32]>::try_from(sl).expect("32b");
    assert_eq!(mmap_h, want);
}

/// **Proves:** CAN-003 test plan `test_set_canonical_block_not_in_store` + error table `BlockNotInStore`.
#[test]
fn test_set_canonical_block_not_in_store() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let unknown = Bytes32::new([0xAB; 32]);
    let err = store.set_canonical(&unknown).expect_err("unknown hash");
    match err {
        BlockStoreError::BlockNotInStore(got) => assert_eq!(got, unknown),
        e => panic!("expected BlockNotInStore, got {e:?}"),
    }
}

/// **Proves:** CAN-003 test plan `test_set_canonical_updates_record` + AC `in_canonical_chain == true`.
#[test]
fn test_set_canonical_updates_record() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(2, Bytes32::default());
    let h = b.hash();
    assert!(store.put_block(&b, false).expect("put"));
    store
        .update_status(&h, BlockStatus::Orphaned)
        .expect("orphan");
    let r = store.get_record(&h).expect("get").expect("rec");
    assert!(!r.in_canonical_chain);
    store.set_canonical(&h).expect("set_canonical");
    let r2 = store.get_record(&h).expect("get2").expect("rec2");
    assert!(r2.in_canonical_chain);
}

/// **Proves:** CAN-003 test plan `test_set_canonical_idempotent` + § Idempotency.
#[test]
fn test_set_canonical_idempotent() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(1, Bytes32::default());
    let h = b.hash();
    assert!(store.put_block(&b, false).expect("put"));
    store.set_canonical(&h).expect("first");
    store.set_canonical(&h).expect("second");
}

/// **Proves:** CAN-003 test plan `test_set_canonical_overwrites_height` + AC “different hash at same height overwrites”.
#[test]
fn test_set_canonical_overwrites_height() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let p0 = Bytes32::default();
    let b_a = test_block(5, p0);
    let p_a = b_a.hash();
    let b_b = test_block(5, p_a);
    let p_b = b_b.hash();
    assert_ne!(p_a, p_b);
    assert!(store.put_block(&b_a, false).expect("put a"));
    assert!(store.put_block(&b_b, false).expect("put b"));
    store.set_canonical(&p_a).expect("canonical a");
    store.set_canonical(&p_b).expect("canonical b overwrites");
    drop(store);

    let got = cf_canonical_hash_at(&path, 5).expect("cf");
    let want_b: [u8; 32] = p_b.as_ref().try_into().expect("hash");
    assert_eq!(got, want_b);
    let bin = std::fs::read(canonical_bin_path(&path)).expect("bin");
    let sl: &[u8] = &bin[(5usize * 32)..(5usize * 32 + 32)];
    assert_eq!(<[u8; 32]>::try_from(sl).unwrap(), want_b);
}
