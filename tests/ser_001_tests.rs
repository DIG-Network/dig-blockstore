//! # SER-001 — Full block serialization: **bincode** + **zstd** (optional trained dictionary)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`SER-001.md`](../docs/requirements/domains/serialization/specs/SER-001.md)
//! - NORMATIVE: [`NORMATIVE.md` (SER-001)](../docs/requirements/domains/serialization/NORMATIVE.md#ser-001-block-serialization-with-dictionary-compression)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/serialization/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **API:** [`BlockStore::serialize_block`] / [`BlockStore::deserialize_block`] ([`dig_blockstore::BlockStore`]) implement
//!   the normative pipeline: `L2Block` → bincode → zstd (dictionary or plain) ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
//! - **Production wiring:** [`BlockStore::init_genesis`] and [`BlockStore::get_block`] delegate to these helpers so `CF_BLOCKS`
//!   values stay consistent ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
//! - **Dictionary:** [`BlockStoreConfig::zstd_dictionary_override`] injects a trained dictionary in tests; on mainnet,
//!   [`META_ZSTD_DICT`](dig_blockstore::META_ZSTD_DICT) loading is implemented for `open` / `open_readonly` when
//!   [`BlockStoreConfig::use_compression_dict`] is true ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) will persist bytes).
//! - **Identity:** [`dig_block::L2Block`] does not implement [`PartialEq`]; equality is checked via [`L2Block::hash`]
//!   (canonical block id per dig-block BLK-003).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::PathBuf;

use chia_protocol::Bytes32;
use dig_block::L2Block;
use dig_blockstore::{
    BlockStore, BlockStoreConfig, BlockStoreError, DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES,
    ZSTD_COMPRESSION_LEVEL,
};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// ZSTD framed block magic (little-endian `0xFD2FB528`) per RFC 8878.
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// True when the frame header declares a **Dictionary ID** (descriptor bit 6 per zstd framing spec).
///
/// **Used for:** SER-001 acceptance §3 — dictionary-compressed frames embed a dictionary id for the decoder.
fn zstd_frame_includes_dictionary_id(frame: &[u8]) -> bool {
    frame.len() >= 5 && frame[..4] == ZSTD_MAGIC && (frame[4] & 0x40) != 0
}

fn assert_same_block(a: &L2Block, b: &L2Block) {
    assert_eq!(
        a.hash(),
        b.hash(),
        "block hash (canonical identity) must match"
    );
}

fn train_dictionary(blocks: &[L2Block]) -> Vec<u8> {
    let samples: Vec<Vec<u8>> = blocks
        .iter()
        .map(|b| bincode::serialize(b).expect("bincode serialize L2Block"))
        .collect();
    let refs: Vec<&[u8]> = samples.iter().map(Vec::as_slice).collect();
    zstd::dict::from_samples(&refs, 64 * 1024).expect("zstd dictionary training")
}

/// Opens a store on a fresh temp path using `cfg_builder`.
fn open_with<F: FnOnce(PathBuf) -> BlockStoreConfig>(
    cfg_builder: F,
) -> (BlockStore, tempfile::TempDir) {
    let (dir, path) = temp_blockstore_dir();
    let cfg = cfg_builder(path);
    let store = BlockStore::open(cfg).expect("BlockStore::open");
    (store, dir)
}

#[test]
fn test_round_trip_plain_zstd_matches_identity() {
    // **Proves:** SER-001 test plan “round-trip” + acceptance §2 — `deserialize_block(serialize_block(b))` preserves hash identity.
    let (store, _dir) = open_with(|path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = false;
        c
    });
    let block = test_block(0, Bytes32::default());
    let wire = store.serialize_block(&block).expect("serialize");
    let back = store.deserialize_block(&wire).expect("deserialize");
    assert_same_block(&block, &back);
}

#[test]
fn test_serialized_payload_is_valid_zstd_frame() {
    // **Proves:** SER-001 acceptance §1 — output is a zstd bitstream [`zstd::decode_all`] can expand to raw bincode.
    let (store, _dir) = open_with(|path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = false;
        c
    });
    let block = test_block(0, Bytes32::default());
    let compressed = store.serialize_block(&block).expect("serialize");
    assert!(
        compressed.starts_with(&ZSTD_MAGIC),
        "zstd magic must prefix CF_BLOCKS value"
    );
    let raw = zstd::decode_all(compressed.as_slice()).expect("plain zstd decode");
    let decoded: L2Block = bincode::deserialize(&raw).expect("bincode");
    assert_same_block(&block, &decoded);
}

#[test]
fn test_compression_ratio_representative_blocks() {
    // **Proves:** SER-001 acceptance §7 + test plan “compression ratio” — typical2–6× for structured bincode payloads.
    let (store, _dir) = open_with(|path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = false;
        c
    });
    let chain = build_chain(12);
    let block = &chain[7];
    let raw_len = bincode::serialize(block).expect("bincode").len();
    let compressed = store.serialize_block(block).expect("serialize");
    let ratio = raw_len as f64 / compressed.len().max(1) as f64;
    assert!(
        (2.0..=6.0).contains(&ratio),
        "expected ~3–5× ratio per SER-001; got {ratio:.2}× (raw {raw_len}, compressed {})",
        compressed.len()
    );
}

#[test]
fn test_dictionary_compression_sets_dictionary_id_bit() {
    // **Proves:** SER-001 acceptance §3 — dictionary mode emits a frame that advertises a Dictionary ID.
    let chain = build_chain(16);
    let dict = train_dictionary(&chain);
    let (store, _dir) = open_with(move |path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = true;
        c.zstd_dictionary_override = Some(dict);
        c.max_decompressed_block_bytes = DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES;
        c
    });
    let block = &chain[10];
    let compressed = store.serialize_block(block).expect("serialize with dict");
    assert!(
        zstd_frame_includes_dictionary_id(&compressed),
        "dictionary-compressed zstd frame should set Dictionary_ID_flag (descriptor bit 6); frame head: {:02x?}",
        &compressed[..compressed.len().min(8)]
    );
    let back = store
        .deserialize_block(&compressed)
        .expect("deserialize with dict");
    assert_same_block(block, &back);
}

#[test]
fn test_plain_fallback_when_dictionary_disabled() {
    // **Proves:** SER-001 acceptance §4 + test plan “plain zstd fallback” — no dictionary → [`zstd::encode_all`] path.
    let (store, _dir) = open_with(|path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = false;
        c.zstd_dictionary_override = None;
        c
    });
    let block = test_block(2, Bytes32::default());
    let compressed = store.serialize_block(&block).expect("serialize");
    assert!(
        !zstd_frame_includes_dictionary_id(&compressed),
        "plain zstd frames in this configuration omit dictionary id"
    );
    let back = store.deserialize_block(&compressed).unwrap();
    assert_same_block(&block, &back);
}

#[test]
fn test_dictionary_session_reads_plain_zstd_fallback() {
    // **Proves:** SER-001 acceptance §5 — dictionary decompress fails → plain [`zstd::decode_all`] succeeds for legacy payloads.
    let chain = build_chain(8);
    let dict = train_dictionary(&chain);
    let (store, _dir) = open_with(move |path| {
        let mut c = test_config(path);
        c.compression_level = 3;
        c.use_compression_dict = true;
        c.zstd_dictionary_override = Some(dict);
        c
    });
    let block = test_block(3, chain[2].hash());
    let raw = bincode::serialize(&block).expect("bincode");
    let plain_only = zstd::encode_all(raw.as_slice(), 3).expect("plain zstd");
    let back = store
        .deserialize_block(&plain_only)
        .expect("fallback plain zstd must succeed");
    assert_same_block(&block, &back);
}

#[test]
fn test_corrupt_compressed_bytes_yield_serialization_error() {
    // **Proves:** SER-001 test plan “corrupted input” — [`BlockStore::deserialize_block`] surfaces [`BlockStoreError::Serialization`].
    let (store, _dir) = open_with(test_config);
    let err = store
        .deserialize_block(&[0xFF, 0xFE, 0xFD, 0xFC])
        .expect_err("garbage must not decode as a block");
    match err {
        BlockStoreError::Serialization(msg) => {
            assert!(
                msg.contains("deserialize_block") || msg.contains("decompress"),
                "message should describe decode failure: {msg}"
            );
        }
        other => panic!("expected Serialization, got {other:?}"),
    }
}

#[test]
fn test_default_compression_level_is_three() {
    // **Proves:** SER-001 acceptance §6 — production default remains level 3 ([`ZSTD_COMPRESSION_LEVEL`] / [`BlockStoreConfig::default`]).
    assert_eq!(ZSTD_COMPRESSION_LEVEL, 3);
    assert_eq!(BlockStoreConfig::default().compression_level, 3);
}
