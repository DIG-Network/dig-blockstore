//! # SER-004 — Round-trip guarantees: bincode, zstd, dictionary zstd, hash invariance
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`SER-004.md`](../docs/requirements/domains/serialization/specs/SER-004.md)
//! - NORMATIVE: [`NORMATIVE.md` (SER-004)](../docs/requirements/domains/serialization/NORMATIVE.md#ser-004-round-trip-guarantees)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/serialization/VERIFICATION.md)
//!
//! ## Scope & exceptions (explicit)
//!
//! - **[`dig_block::L2Block`]** — does **not** implement [`PartialEq`]; consensus identity is [`L2Block::hash`].
//!   We prove bincode stability via **byte-for-byte** `bincode::serialize` equality after a deserialize round-trip
//!   ([`SER-004`](../docs/requirements/domains/serialization/specs/SER-004.md) AC §1 + hash match).
//! - **[`BlockRecord`](dig_blockstore::BlockRecord)** — [`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) **forbids**
//!   `serde::Serialize` to prevent accidental RocksDB persistence. **Bincode round-trip is N/A**; we prove
//!   **in-memory identity** via [`PartialEq`] + [`Clone`] ([`SER-004`](../docs/requirements/domains/serialization/specs/SER-004.md) AC §6 note).
//! - **`SnapshotManifest`** — listed in SER-004 AC §6 but **not yet defined** in this crate (see [`SNP-003`](../docs/requirements/domains/snapshot/specs/SNP-003.md) / `src/snapshot.rs` placeholder).
//!   When the type lands, extend this file with `bincode` round-trip like [`AttestedBlock`].
//! - **Property tests:** SER-004 implementation notes suggest `proptest`; we use **deterministic multi-block chains**
//!   ([`build_chain`](common::build_chain)) to avoid new dev-dependencies while still covering several shapes.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::PathBuf;

use chia_protocol::Bytes32;
use dig_block::{AttestedBlock, BlockStatus, L2Block, ReceiptList};
use dig_blockstore::{
    BlockRecord, BlockStore, BlockStoreConfig, DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES,
    ZSTD_COMPRESSION_LEVEL,
};

use common::{build_chain, temp_blockstore_dir, test_block, test_config, test_header};

// ----------------------------------------------------------------------------- bincode helpers

/// **Proves:** [`bincode`] is deterministic for `T` and round-trip preserves value ([`SER-004`](../docs/requirements/domains/serialization/specs/SER-004.md) spec snippet).
fn assert_bincode_round_trip_partial_eq<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let bytes = bincode::serialize(value).expect("bincode::serialize");
    let restored: T = bincode::deserialize(&bytes).expect("bincode::deserialize");
    assert_eq!(*value, restored, "deserialize(serialize(x)) must equal x");
}

/// **Proves:** Same logical value after bincode cycle for types **without** [`PartialEq`] — stable canonical bytes + identity hash.
///
/// Used for [`L2Block`] ([`L2Block::hash`]) and [`AttestedBlock`] ([`AttestedBlock::hash`]).
fn assert_bincode_stable_identity<H, F>(value: &H, hash: F)
where
    H: serde::Serialize + serde::de::DeserializeOwned,
    F: Fn(&H) -> Bytes32,
{
    let bytes = bincode::serialize(value).expect("serialize");
    let restored: H = bincode::deserialize(&bytes).expect("deserialize");
    let again = bincode::serialize(&restored).expect("re-serialize");
    assert_eq!(
        bytes, again,
        "bincode encoding must be stable across one round-trip"
    );
    assert_eq!(
        hash(value),
        hash(&restored),
        "canonical hash must match after bincode round-trip"
    );
}

// ----------------------------------------------------------------------------- zstd helpers (spec listing)

fn assert_zstd_plain_round_trip(data: &[u8], level: i32) {
    let compressed = zstd::encode_all(data, level).expect("encode_all");
    let decompressed = zstd::decode_all(&compressed[..]).expect("decode_all");
    assert_eq!(data, decompressed.as_slice());
}

fn assert_zstd_dictionary_round_trip(data: &[u8], dict: &[u8], max_out: usize) {
    let mut compressor =
        zstd::bulk::Compressor::with_dictionary(ZSTD_COMPRESSION_LEVEL, dict).expect("compressor");
    let compressed = compressor.compress(data).expect("compress with dict");
    let mut decompressor =
        zstd::bulk::Decompressor::with_dictionary(dict).expect("decompressor");
    let out = decompressor
        .decompress(compressed.as_slice(), max_out)
        .expect("decompress with dict");
    assert_eq!(data, out.as_slice());
}

/// Train a zstd dictionary from bincode-serialized blocks — same sizing as [`SER-001`](ser_001_tests.rs) (`from_samples` needs enough total bytes).
fn train_dict_from_blocks(blocks: &[L2Block]) -> Vec<u8> {
    let samples: Vec<Vec<u8>> = blocks
        .iter()
        .map(|b| bincode::serialize(b).expect("bincode L2Block"))
        .collect();
    let refs: Vec<&[u8]> = samples.iter().map(Vec::as_slice).collect();
    zstd::dict::from_samples(&refs, 64 * 1024).expect("zstd dictionary training")
}

fn open_store_with_cfg(path: PathBuf, f: impl FnOnce(&mut BlockStoreConfig)) -> BlockStore {
    let mut cfg = test_config(path);
    f(&mut cfg);
    BlockStore::open(cfg).expect("BlockStore::open")
}

#[test]
fn test_bincode_round_trip_l2_block_header_partial_eq() {
    // **Proves:** AC §2 — [`L2BlockHeader`] is [`PartialEq`] + serde; strict value equality after bincode.
    let h = test_header(11, Bytes32::default());
    assert_bincode_round_trip_partial_eq(&h);
}

#[test]
fn test_bincode_round_trip_l2_block_stable_bytes_and_hash() {
    // **Proves:** AC §1 — [`L2Block`] bincode round-trip preserves canonical identity ([`L2Block::hash`]).
    let block = test_block(4, Bytes32::default());
    assert_bincode_stable_identity(&block, L2Block::hash);
}

#[test]
fn test_bincode_round_trip_attested_block_stable_bytes_and_hash() {
    // **Proves:** AC §6 — [`AttestedBlock`] (serde) round-trip; identity via [`AttestedBlock::hash`] (delegates to block).
    let block = test_block(0, Bytes32::default());
    let ab = AttestedBlock::new(block, 8, ReceiptList::default());
    assert_bincode_stable_identity(&ab, AttestedBlock::hash);
}

#[test]
fn test_block_record_clone_identity_without_bincode() {
    // **Proves:** AC §6 for [`BlockRecord`] — **no bincode** path ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) forbids `Serialize`).
    // In-memory equality is still a round-trip identity for cache-resident records.
    let header = test_header(2, Bytes32::default());
    let r = BlockRecord::from_header(&header, BlockStatus::Validated);
    assert_eq!(r, r.clone(), "clone round-trip must preserve BlockRecord");
}

#[test]
fn test_zstd_plain_round_trip_random_payloads() {
    // **Proves:** AC §3 — plain zstd is byte-identical for representative payloads at level 3 ([`ZSTD_COMPRESSION_LEVEL`](dig_blockstore::ZSTD_COMPRESSION_LEVEL)).
    for data in [
        &b""[..],
        b"short",
        &[0u8; 4096],
        include_bytes!("ser_004_tests.rs").as_slice(),
    ] {
        assert_zstd_plain_round_trip(data, ZSTD_COMPRESSION_LEVEL);
    }
}

#[test]
fn test_zstd_dictionary_round_trip_on_serialized_blocks() {
    // **Proves:** AC §4 — dictionary compressor/decompressor recover exact **pre-zstd** bytes (here: bincode `L2Block` bodies).
    let chain = build_chain(16);
    let dict = train_dict_from_blocks(&chain);
    for b in &chain {
        let s = bincode::serialize(b).expect("bincode");
        assert_zstd_dictionary_round_trip(&s, &dict, s.len().saturating_mul(4).max(1024));
    }
}

#[test]
fn test_hash_invariance_blockstore_plain_zstd_pipeline() {
    // **Proves:** AC §5 — full [`BlockStore::serialize_block`] / [`BlockStore::deserialize_block`] path preserves [`L2BlockHeader::hash`].
    let (_dir, path) = temp_blockstore_dir();
    let store = open_store_with_cfg(path, |c| {
        c.use_compression_dict = false;
        c.compression_level = 3;
    });
    let block = test_block(1, Bytes32::default());
    let h0 = block.hash();
    let bytes = store.serialize_block(&block).expect("serialize_block");
    let back = store.deserialize_block(&bytes).expect("deserialize_block");
    assert_eq!(h0, back.hash(), "hash invariance (plain zstd pipeline)");
}

#[test]
fn test_hash_invariance_with_trained_dictionary_override() {
    // **Proves:** AC §4 + §5 — dictionary path still preserves header hash ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) interaction).
    let chain = build_chain(12);
    let dict = train_dict_from_blocks(&chain);

    let (_dir, path) = temp_blockstore_dir();
    let store = open_store_with_cfg(path, |c| {
        c.use_compression_dict = true;
        c.zstd_dictionary_override = Some(dict);
        c.max_decompressed_block_bytes = DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES;
        c.compression_level = 3;
    });

    let block = test_block(3, Bytes32::default());
    let h0 = block.hash();
    let raw = store.serialize_block(&block).expect("serialize with dict");
    let back = store.deserialize_block(&raw).expect("deserialize");
    assert_eq!(h0, back.hash(), "hash invariance (dictionary zstd pipeline)");
}

#[test]
fn test_deterministic_chain_blocks_bincode_and_store_hashes() {
    // **Proves:** SER-004 test plan “arbitrary blocks” substitute — several heights from [`build_chain`] all satisfy
    // bincode stability + store pipeline hash invariance.
    let (_dir, path) = temp_blockstore_dir();
    let store = open_store_with_cfg(path, |c| {
        c.use_compression_dict = false;
        c.compression_level = ZSTD_COMPRESSION_LEVEL;
    });

    for block in build_chain(7) {
        assert_bincode_stable_identity(&block, L2Block::hash);
        let h = block.hash();
        let blob = store.serialize_block(&block).unwrap();
        let got = store.deserialize_block(&blob).unwrap();
        assert_eq!(h, got.hash());
    }
}
