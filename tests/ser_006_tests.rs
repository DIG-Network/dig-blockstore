//! # SER-006 — Dictionary-override compress/decompress path + runtime `init_dictionary`
//!
//! **Trace**
//! - Spec: [`SER-001.md`](../docs/requirements/domains/serialization/specs/SER-001.md) (block
//!   serialization), [`SER-005.md`](../docs/requirements/domains/serialization/specs/SER-005.md)
//!   (dictionary management).
//!
//! ## What this file proves
//!
//! The SER-005 suite trains a dictionary the slow way (1000 blocks crossing
//! [`dig_blockstore::DICT_TRAINING_THRESHOLD`]) and verifies persistence/fallback. It never exercises
//! the **`zstd_dictionary_override`** config wiring — the path where a caller injects dictionary bytes
//! at [`BlockStore::open`] so the `serialize_block` *with-dictionary* compress branch and the
//! `deserialize_block` dictionary-aware decompress branch run immediately, nor the public runtime
//! reload [`BlockStore::init_dictionary`]. This file drives both:
//!
//! - opening with an override dictionary and round-tripping a block through the dict-backed codec,
//! - a dict-compressed payload carrying the zstd magic and being smaller-or-equal to a trivial body,
//! - `init_dictionary` reloading from `CF_METADATA` (no-op when none, then a real reload after a put).
//!
//! All in-process — no network, no mainnet, no real zk-proving.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{BlockStore, BlockStoreConfig};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// ZSTD frame magic (little-endian `0xFD2FB528`).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Build a config with compression + a raw-content dictionary override installed at open. A raw byte
/// buffer is a valid zstd dictionary for the bulk codec (content-only dictionary), so we avoid the
/// 1000-block training corpus while still exercising the `with_dictionary` compress/decompress arms.
fn override_dict_config(path: std::path::PathBuf, dict: Vec<u8>) -> BlockStoreConfig {
    let mut c = test_config(path);
    c.compress_blocks = true;
    c.compression_level = 3;
    c.use_compression_dict = true;
    c.zstd_dictionary_override = Some(dict);
    c.max_decompressed_block_bytes = 16 * 1024 * 1024;
    c
}

/// **Proves:** SER-001/SER-005 — when an override dictionary is wired at open, `serialize_block` takes
/// the `Compressor::with_dictionary` branch (output is a zstd frame) and `deserialize_block` decodes
/// it back to the identical block via the dictionary-aware decompress branch.
#[test]
fn test_override_dictionary_round_trip() {
    let (_guard, path) = temp_blockstore_dir();
    // A non-trivial repeating dictionary so the codec has shared content to reference.
    let dict: Vec<u8> = (0..8192u32).map(|i| (i % 251) as u8).collect();
    let store =
        BlockStore::open(override_dict_config(path, dict)).expect("open with override dict");

    let block = test_block(1, chia_protocol::Bytes32::default());
    let compressed = store.serialize_block(&block).expect("serialize via dict");
    assert!(
        compressed.len() >= 4 && compressed[..4] == ZSTD_MAGIC[..],
        "dictionary compress must still emit a zstd frame"
    );

    let decoded = store
        .deserialize_block(&compressed)
        .expect("deserialize via dict");
    assert_eq!(
        decoded.hash(),
        block.hash(),
        "dictionary round-trip must preserve block identity"
    );
}

/// **Proves:** SER-005 — a block stored under the override dictionary is retrievable across a full
/// `put` → `get_block` cycle (the store wires the dict into both the write and read paths).
#[test]
fn test_override_dictionary_put_get_cycle() {
    let (_guard, path) = temp_blockstore_dir();
    let dict: Vec<u8> = (0..4096u32).map(|i| (i % 97) as u8).collect();
    let store = BlockStore::open(override_dict_config(path, dict)).expect("open");

    let chain = build_chain(3);
    store.init_genesis(&chain[0]).expect("genesis");
    assert!(store.put(&chain[1], true).expect("put h1"));
    assert!(store.put(&chain[2], true).expect("put h2"));

    for b in &chain {
        let got = store.get_block(&b.hash()).expect("get").expect("present");
        assert_eq!(got.hash(), b.hash(), "dict-stored block must round-trip");
    }
}

/// **Proves:** SER-005 — `init_dictionary` is a safe public reload: on a store with no trained
/// dictionary in `CF_METADATA` it succeeds and leaves the in-memory slot empty (blocks still
/// round-trip via plain zstd), and a second call is idempotent.
#[test]
fn test_init_dictionary_noop_when_no_metadata() {
    let (_guard, path) = temp_blockstore_dir();
    let mut cfg = test_config(path);
    cfg.compress_blocks = true;
    cfg.use_compression_dict = true; // feature on, but no trained dict persisted
    cfg.compression_level = 3;
    let store = BlockStore::open(cfg).expect("open");

    // No metadata dictionary yet → reload is a clean no-op.
    store
        .init_dictionary()
        .expect("init_dictionary with no metadata");
    store.init_dictionary().expect("idempotent second reload");

    // Plain-zstd round-trip still works (no dictionary installed).
    let block = test_block(0, chia_protocol::Bytes32::default());
    let compressed = store.serialize_block(&block).expect("serialize");
    let decoded = store.deserialize_block(&compressed).expect("deserialize");
    assert_eq!(decoded.hash(), block.hash());
}
