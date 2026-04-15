//! # SER-005 — Zstd dictionary **training**, **persistence**, and **plain fallback**
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`SER-005.md`](../docs/requirements/domains/serialization/specs/SER-005.md)
//! - NORMATIVE: [`NORMATIVE.md` (SER-005)](../docs/requirements/domains/serialization/NORMATIVE.md#ser-005-dictionary-training-and-management)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/serialization/VERIFICATION.md)
//!
//! ## What this file proves (acceptance criteria mapping)
//!
//! | AC | Idea | Primary test |
//! |----|------|----------------|
//! | 1 | Fresh DB, dictionary feature on but no blob in [`CF_METADATA`] → compressor uses **plain zstd** | [`test_fresh_startup_plain_zstd_no_metadata_dictionary`] |
//! | 2 | After the block whose insert makes [`DICT_TRAINING_THRESHOLD`] total rows, training runs | [`test_training_triggers_when_block_count_reaches_threshold`] |
//! | 3 | Training uses `zstd::dict::from_samples` semantics (persisted blob is a valid dict for bulk codec) | [`test_training_triggers_when_block_count_reaches_threshold`], [`test_dictionary_size_within_spec_band`] |
//! | 4 | Bytes land under [`META_ZSTD_DICT`] | [`test_training_triggers_when_block_count_reaches_threshold`], [`read_meta_zstd_dict`] |
//! | 5 | Re-open loads dictionary into [`BlockStore`] so dict-backed compress/decompress works | [`test_dictionary_persists_and_loads_on_reopen`] |
//! | 6 | Rows written **before** training (plain zstd) stay readable via [`BlockStore::deserialize_block`] fallback | [`test_pre_dictionary_blocks_readable_after_training`] |
//! | 7 | Trained blob ~100 KB per [`DICT_TARGET_SIZE`] | [`test_dictionary_size_within_spec_band`] |
//!
//! **Additional proof:** mixed-mode corpus + one post-train block ([`test_mixed_mode_reads_all_round_trip`]), read-only guard for [`BlockStore::put`] ([`test_put_on_readonly_store_errors`]), idempotency that training does not rewrite metadata on every subsequent put ([`test_no_double_training_after_dictionary_installed`]).
//!
//! **Dependency note:** [`BlockStore::put`] is introduced here as the **BLK-001-shaped** write path required to reach 1000 stored blocks; full BLK-001 acceptance tests remain in future `tests/blk_001_tests.rs`.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use dig_block::L2Block;
use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::{
    BlockStore, BlockStoreConfig, BlockStoreError, CF_METADATA, DICT_TARGET_SIZE,
    DICT_TRAINING_THRESHOLD, ERR_MUTATION_READ_ONLY, META_ZSTD_DICT,
};

use common::{build_chain, temp_blockstore_dir, test_config};

/// ZSTD framed block magic (little-endian `0xFD2FB528`) — proves payloads are zstd frames ([`SER-001`](ser_001_tests.rs)).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

fn open_opts() -> rocksdb::Options {
    let mut o = rocksdb::Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Configuration for SER-005: **must** enable [`BlockStoreConfig::use_compression_dict`] so the training gate in
/// [`BlockStore::maybe_train_dictionary`] (`store.rs`) is active; production-like level + realistic decompressed cap.
fn ser005_config(path: std::path::PathBuf) -> BlockStoreConfig {
    let mut c = test_config(path);
    c.use_compression_dict = true;
    c.compression_level = 3;
    c.max_decompressed_block_bytes = 16 * 1024 * 1024;
    c
}

/// Read raw [`META_ZSTD_DICT`] bytes **after** dropping all [`BlockStore`] handles — proves on-disk persistence ([`SER-005`] AC §4).
fn read_meta_zstd_dict(path: &Path) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| rocksdb::ColumnFamilyDescriptor::new(*n, rocksdb::Options::default()))
        .collect();
    let db = rocksdb::DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_METADATA)?;
    db.get_cf(cf, META_ZSTD_DICT.as_bytes()).ok().flatten()
}

/// Materialize `total` blocks: genesis via [`BlockStore::init_genesis`], then [`BlockStore::put`] for the tail so the
/// last `put` is the `total`-th row in [`CF_BLOCKS`].
fn fill_blocks_up_to(store: &BlockStore, chain: &[L2Block], total: usize) {
    assert!(
        chain.len() >= total,
        "chain must cover at least {total} heights"
    );
    store.init_genesis(&chain[0]).expect("init_genesis");
    for block in chain.iter().take(total).skip(1) {
        assert!(
            store.put(block, true).expect("put block"),
            "every height in 1..{total} must insert a novel hash"
        );
    }
    assert_eq!(
        store.block_count().expect("block_count"),
        total as u64,
        "sanity: CF_BLOCKS row count matches inserts"
    );
}

#[test]
fn test_fresh_startup_plain_zstd_no_metadata_dictionary() {
    // **Proves:** SER-005 AC §1 — no [`META_ZSTD_DICT`] row ⇒ in-memory slot empty at [`BlockStore::open`];
    // [`serialize_block`] falls through to [`zstd::encode_all`] ([`store.rs`] `serialize_block`).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    assert!(
        read_meta_zstd_dict(path.as_path()).is_none(),
        "brand-new store must not have trained dictionary metadata"
    );
    let chain = build_chain(2);
    store.init_genesis(&chain[0]).expect("init_genesis");
    let compressed = store.serialize_block(&chain[1]).expect("serialize_block");
    assert!(
        compressed.len() >= 4 && compressed[..4] == ZSTD_MAGIC[..],
        "plain zstd frame must carry the standard magic prefix"
    );
    assert!(
        store.put(&chain[1], true).expect("put"),
        "first extension of canonical chain must insert a new hash"
    );
    let back = store
        .get_block(&chain[1].hash())
        .expect("get_block")
        .expect("block present");
    assert_eq!(
        back.hash(),
        chain[1].hash(),
        "round-trip identity uses L2Block::hash (SER-004 pattern)"
    );
}

#[test]
fn test_training_triggers_when_block_count_reaches_threshold() {
    // **Proves:** AC §2–§4 — exactly when [`DICT_TRAINING_THRESHOLD`] rows exist, training runs and writes [`META_ZSTD_DICT`].
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize);
    fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    let dict =
        read_meta_zstd_dict(path.as_path()).expect("dictionary must be persisted after training");
    assert!(
        !dict.is_empty(),
        "META_ZSTD_DICT must be non-empty once training completes"
    );
    // Prove the blob is a real zstd dictionary for the bulk API (from_samples output contract).
    let _c = zstd::bulk::Compressor::with_dictionary(3, dict.as_slice())
        .expect("valid trained dictionary bytes");
}

#[test]
fn test_dictionary_persists_and_loads_on_reopen() {
    // **Proves:** AC §5 — second process / new handle reads metadata and dictionary mode engages without errors.
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(ser005_config(path.clone())).expect("open");
        let chain = build_chain(DICT_TRAINING_THRESHOLD as usize);
        fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    }
    assert!(
        read_meta_zstd_dict(path.as_path()).is_some(),
        "dictionary must survive closing the DB"
    );
    let store2 = BlockStore::open(ser005_config(path.clone())).expect("reopen");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize + 1);
    let h = chain[DICT_TRAINING_THRESHOLD as usize].hash();
    assert!(
        store2
            .put(&chain[DICT_TRAINING_THRESHOLD as usize], true)
            .expect("put"),
        "post-reopen put must see a novel hash"
    );
    let blk = store2.get_block(&h).expect("get_block").expect("present");
    assert_eq!(blk.hash(), h);
}

#[test]
fn test_pre_dictionary_blocks_readable_after_training() {
    // **Proves:** AC §6 — genesis row was compressed **before** any dictionary existed; after training + memory install,
    // [`deserialize_block`] still decodes via plain-zstd fallback ([`store.rs`] `decompress_block_payload`).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize);
    let genesis_hash = chain[0].hash();
    fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    let genesis_after = store
        .get_block(&genesis_hash)
        .expect("get_block")
        .expect("genesis must remain readable");
    assert_eq!(genesis_after.hash(), genesis_hash);
}

#[test]
fn test_dictionary_size_within_spec_band() {
    // **Proves:** AC §7 / implementation notes — target is ~[`DICT_TARGET_SIZE`]; tolerate codec variance with a band.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize);
    fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    let dict = read_meta_zstd_dict(path.as_path()).expect("dict");
    let lo = 50 * 1024;
    let hi = 150 * 1024;
    assert!(
        dict.len() >= lo && dict.len() <= hi,
        "trained dictionary should sit near {} bytes (got {} bytes)",
        DICT_TARGET_SIZE,
        dict.len()
    );
}

#[test]
fn test_no_double_training_after_dictionary_installed() {
    // **Proves:** SER-005 test plan “no double training” — once metadata is populated and memory holds [`Some`],
    // subsequent [`put`] calls must not grow or replace the blob arbitrarily (snapshot before/after extra puts).
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize + 5);
    fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    let dict_before = read_meta_zstd_dict(path.as_path()).expect("dict");
    for block in chain
        .iter()
        .skip(DICT_TRAINING_THRESHOLD as usize)
    {
        assert!(
            store.put(block, true).expect("put ok"),
            "row must be novel"
        );
    }
    let dict_after = read_meta_zstd_dict(path.as_path()).expect("dict still present");
    assert_eq!(
        dict_before, dict_after,
        "maybe_train_dictionary should become a no-op once dictionary exists (spec: one-time training)"
    );
}

#[test]
fn test_mixed_mode_reads_all_round_trip() {
    // **Proves:** integration test plan “mixed-mode reads” — corpus includes pre- and post-dictionary frames in one session.
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(ser005_config(path.clone())).expect("open");
    let chain = build_chain(DICT_TRAINING_THRESHOLD as usize + 3);
    fill_blocks_up_to(&store, &chain, DICT_TRAINING_THRESHOLD as usize);
    for block in chain
        .iter()
        .skip(DICT_TRAINING_THRESHOLD as usize)
    {
        assert!(
            store.put(block, true).expect("put"),
            "post-training extension must remain idempotent on duplicates=false insert path"
        );
    }
    for b in &chain {
        let got = store.get_block(&b.hash()).expect("get_block").expect("row");
        assert_eq!(got.hash(), b.hash());
    }
}

#[test]
fn test_put_on_readonly_store_errors() {
    // **Proves:** [`BlockStore::put`] respects the read-only guard (stable [`ERR_MUTATION_READ_ONLY`] text).
    let (_guard, path) = temp_blockstore_dir();
    {
        let store = BlockStore::open(ser005_config(path.clone())).expect("open");
        let chain = build_chain(2);
        store.init_genesis(&chain[0]).expect("init_genesis");
    }
    let ro = BlockStore::open_readonly(path.as_path()).expect("open_readonly");
    let chain = build_chain(2);
    let err = ro
        .put(&chain[1], true)
        .expect_err("read-only put must fail");
    match err {
        BlockStoreError::Serialization(msg) => assert!(
            msg.contains(ERR_MUTATION_READ_ONLY),
            "expected {ERR_MUTATION_READ_ONLY}, got {msg}"
        ),
        other => panic!("unexpected error variant: {other:?}"),
    }
}
