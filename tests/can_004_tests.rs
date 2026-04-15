//! # CAN-004 — `set_canonical_batch` (atomic multi-height canonical promotion)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`CAN-004.md`](../docs/requirements/domains/canonical_chain/specs/CAN-004.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-004)](../docs/requirements/domains/canonical_chain/NORMATIVE.md#can-004-set_canonical_batch)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! [`CAN-004`](../../docs/requirements/domains/canonical_chain/specs/CAN-004.md) requires [`BlockStore::set_canonical_batch`](dig_blockstore::BlockStore::set_canonical_batch) to:
//!
//! - **Validate before write:** If any hash is missing from the store, return [`BlockStoreError::BlockNotInStore`](dig_blockstore::BlockStoreError::BlockNotInStore) and perform **no** RocksDB canonical mutations (fail-fast preserves the “no partial `CF_CANONICAL` from this API” story even though RocksDB batches are already atomic once started).
//! - **Single `WriteBatch`:** After validation, all height→hash rows for the batch are committed with one [`rocksdb::DB::write`](https://docs.rs/rocksdb/latest/rocksdb/struct.DB.html#method.write) so observers never see a half-applied reorg tip.
//! - **Dual layer after success:** Same post-commit path as [`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md) — `canonical.bin` slots and [`BlockRecord::in_canonical_chain`](dig_blockstore::BlockRecord::in_canonical_chain) in the record cache.
//! - **Empty input:** `&[]` succeeds as a no-op.
//! - **Read-only stores:** Reject with the same serialization/`ERR_MUTATION_READ_ONLY` surface as other mutating APIs ([`STR-004`](../../docs/requirements/domains/crate_structure/specs/STR-004.md) parity with [`tests/blk_009_tests.rs`](blk_009_tests.rs)).
//!
//! **How we probe `CF_CANONICAL`:** After dropping the owning [`BlockStore`], we open a standalone [`rocksdb::DB`](https://docs.rs/rocksdb/latest/rocksdb/struct.DB.html) on the same path so Windows releases the lock ([`tests/can_003_tests.rs`](can_003_tests.rs) pattern).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::BlockStatus;
use dig_block::constants::ZERO_HASH;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::encoding::height_key;
use dig_blockstore::{
    BlockStore, BlockStoreError, CF_CANONICAL, ERR_MUTATION_READ_ONLY,
};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

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

/// **Proves:** CAN-004 test plan `test_batch_all_canonical` + AC “after success, all hashes are canonical in both CF and mmap”.
#[test]
fn test_set_canonical_batch_all_canonical() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let chain = build_chain(10);
    let mut hashes = Vec::new();
    for b in &chain {
        assert!(store.put_block(b, false).expect("put"));
        hashes.push(b.hash());
    }
    store
        .set_canonical_batch(&hashes)
        .expect("set_canonical_batch");
    drop(store);

    for (i, h) in hashes.iter().enumerate() {
        let want: [u8; 32] = h.as_ref().try_into().expect("hash bytes");
        let got = cf_canonical_hash_at(&path, i as u64).expect("cf row");
        assert_eq!(got, want, "height {i}");
        let bin = std::fs::read(canonical_bin_path(&path)).expect("bin");
        let start = i * 32;
        let mmap_h: [u8; 32] = bin[start..start + 32].try_into().expect("32b");
        assert_eq!(mmap_h, want, "mmap height {i}");
    }
}

/// **Proves:** CAN-004 AC “empty input slice is a no-op”.
#[test]
fn test_set_canonical_batch_empty_input() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    store.set_canonical_batch(&[]).expect("empty batch");
    drop(store);
    assert!(
        cf_canonical_hash_at(&path, 0).is_none(),
        "no canonical rows should appear from an empty batch"
    );
}

/// **Proves:** CAN-004 test plan `test_batch_fails_on_missing_block` + AC “batch with one invalid hash in the middle fails with no partial writes”.
#[test]
fn test_set_canonical_batch_missing_hash_no_partial_cf() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path.clone())).expect("open");
    let b0 = test_block(0, Bytes32::default());
    let b1 = test_block(1, b0.hash());
    let b2 = test_block(2, b1.hash());
    let h0 = b0.hash();
    let h1 = b1.hash();
    let _h2 = b2.hash();
    store.put_block(&b0, false).expect("put0");
    store.put_block(&b1, false).expect("put1");
    store.put_block(&b2, false).expect("put2");
    store.set_canonical(&h0).expect("seed height 0 only");
    let unknown = Bytes32::new([0xCD; 32]);
    let err = store
        .set_canonical_batch(&[h0, unknown, h1])
        .expect_err("unknown middle hash");
    match err {
        BlockStoreError::BlockNotInStore(got) => assert_eq!(got, unknown),
        e => panic!("expected BlockNotInStore, got {e:?}"),
    }
    drop(store);
    let h0_cf = cf_canonical_hash_at(&path, 0).expect("h0 still canonical");
    let want0: [u8; 32] = h0.as_ref().try_into().expect("bytes");
    assert_eq!(h0_cf, want0);
    assert!(
        cf_canonical_hash_at(&path, 1).is_none(),
        "height 1 must not be written when validation fails before WriteBatch"
    );
    assert!(
        cf_canonical_hash_at(&path, 2).is_none(),
        "height 2 must not be written"
    );
}

/// **Proves:** CAN-004 test plan `test_batch_record_updates` — all touched records flip `in_canonical_chain` after one batch.
#[test]
fn test_set_canonical_batch_updates_record_flags() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let chain = build_chain(4);
    let hashes: Vec<_> = chain
        .iter()
        .map(|b| {
            assert!(store.put_block(b, false).expect("put"));
            let h = b.hash();
            store
                .update_status(&h, BlockStatus::Orphaned)
                .expect("orphan");
            h
        })
        .collect();
    store
        .set_canonical_batch(&hashes)
        .expect("set_canonical_batch");
    for h in &hashes {
        let r = store.get_record(h).expect("get").expect("rec");
        assert!(r.in_canonical_chain, "hash {h:?}");
    }
}

/// **Proves:** Read-only parity with other mutators — batch must not touch DB when `read_only` ([`ERR_MUTATION_READ_ONLY`](dig_blockstore::ERR_MUTATION_READ_ONLY)).
#[test]
fn test_set_canonical_batch_read_only_rejected() {
    let (_guard, path) = temp_blockstore_dir();
    let path_ro = path.clone();
    let h = {
        let s = BlockStore::open(test_config(path)).expect("open rw");
        let b = test_block(0, ZERO_HASH);
        let h = b.hash();
        s.put_block(&b, false).expect("seed");
        h
    };
    let ro = BlockStore::open_readonly(path_ro).expect("ro");
    let err = ro
        .set_canonical_batch(&[h])
        .expect_err("read-only must error");
    match err {
        BlockStoreError::Serialization(s) => assert!(
            s.contains(ERR_MUTATION_READ_ONLY) || s.contains("read-only") || s.contains("read only"),
            "{s}"
        ),
        other => panic!("unexpected {other:?}"),
    }
}
