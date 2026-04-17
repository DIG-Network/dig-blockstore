//! # dig-blockstore
//!
//! Persistent storage for validated DIG L2 blocks: RocksDB layout, canonical
//! height→hash indexing, caching, rollback, pruning, and checkpoints — see
//! `docs/resources/SPEC.md` for the authoritative architecture.
//!
//! ## Module layout (STR-002)
//!
//! The subtree under `src/` mirrors [`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md)
//! and **§16 — Crate boundary** in `docs/resources/SPEC.md`: store, config, types,
//! constants, [`cf_options`](crate::cf_options) ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)),
//! errors, encoding, cache, canonical index, compression, async pipeline,
//! and snapshot I/O.
//!
//! ## Public API surface (STR-003)
//!
//! Primary types, constants, and key helpers are re-exported at the crate root per
//! [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md) and **§15** in
//! `docs/resources/SPEC.md`. Submodules remain `pub` so integration tests and advanced callers
//! can use qualified paths (e.g. [`store::BlockStore`](crate::store::BlockStore)).
//!
//! ## STR-001 dependency smoke
//!
//! [`str001_dependency_smoke`] forces every direct `Cargo.toml` dependency to link,
//! catching resolution failures early ([`STR-001`](../docs/requirements/domains/crate_structure/specs/STR-001.md)).
// `memmap2` mapping constructors are `unsafe` ([`CAN-001`](docs/requirements/domains/canonical_chain/specs/CAN-001.md));
// isolated, documented `unsafe` lives in [`crate::canonical::mmap`]. Use `deny` (not `forbid`) so that module can opt in.
#![deny(unsafe_code)]

pub mod cache;
pub mod canonical;
pub mod cf_options;
pub mod compression;
pub mod config;
pub mod constants;
pub mod encoding;
pub mod error;
pub mod pipeline;
pub mod snapshot;
pub mod store;
pub mod types;
/// Chia [`Streamable`] wire serialization for [`dig_block::L2Block`] ([`SER-003`](../docs/requirements/domains/serialization/specs/SER-003.md)).
pub mod wire;

// --- STR-003 + API-001: flat public API (`use dig_blockstore::{…}`) ---
//
// Re-export upstream types that appear in public method signatures so consumers
// can write `use dig_blockstore::{BlockStore, L2Block, Bytes32}` without adding
// dig-block or chia-protocol to their own Cargo.toml.
pub use chia_protocol::Bytes32;
pub use config::BlockStoreConfig;
pub use constants::{
    CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS, CF_METADATA,
    DEFAULT_BLOCK_CACHE_CAPACITY, DEFAULT_BLOCK_CACHE_SIZE, DEFAULT_BLOOM_BITS_PER_KEY,
    DEFAULT_HEADER_CACHE_CAPACITY, DEFAULT_MAX_DECOMPRESSED_BLOCK_BYTES, DEFAULT_MAX_OPEN_FILES,
    DEFAULT_WRITE_BUFFER_SIZE, DICT_TARGET_SIZE, DICT_TRAINING_THRESHOLD, META_GENESIS_HASH,
    META_MIN_HEIGHT, META_SCHEMA_VERSION, META_TIP, META_ZSTD_DICT, SCHEMA_VERSION,
    ZSTD_COMPRESSION_LEVEL,
};
pub use dig_block::{AttestedBlock, BlockStatus, L2Block, L2BlockHeader};
pub use encoding::{
    decode_epoch_key, decode_height_key, epoch_key, hash_key, height_key, metadata_key,
};
pub use error::{
    BlockStoreError, ERR_ASYNC_JOIN_PREFIX, ERR_INIT_GENESIS_ALREADY_INITIALIZED,
    ERR_INIT_GENESIS_READ_ONLY, ERR_MUTATION_READ_ONLY, ERR_OPEN_READONLY_PATH_MISSING_PREFIX,
    ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX,
};
pub use snapshot::{SnapshotManifest, SNAPSHOT_VERSION};
pub use store::{BlockStore, StreamBlocksInRange};
pub use types::{BlockRecord, ChainTip, ReorgResult, StorageStats, StoredCheckpoint};
pub use wire::{block_from_wire_bytes, block_to_wire_bytes};

// --- API-003: Compile-time thread-safety assertions ---
// BlockStore is designed for concurrent use across tokio tasks and thread pools.
// All public methods take &self. Clone is cheap (Arc<Inner>).
const _: () = {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    fn _assert_clone<T: Clone>() {}
    fn _assertions() {
        _assert_send::<BlockStore>();
        _assert_sync::<BlockStore>();
        _assert_clone::<BlockStore>();
        _assert_send::<BlockStoreError>();
        _assert_sync::<BlockStoreError>();
    }
};

// L2BlockHeader re-exported above via `pub use dig_block::...`
use dig_constants::NetworkConstants;
use dig_epoch::BLOCKS_PER_EPOCH;

/// Exercises every **direct** `[dependencies]` entry from STR-001 so `cargo check`
/// cannot succeed if any crate fails to resolve.
///
/// **Semantic links:**
/// - Requirement & test plan: `docs/requirements/domains/crate_structure/specs/STR-001.md`
/// - Normative summary: `docs/requirements/domains/crate_structure/NORMATIVE.md`
/// - Spec §1.2: `docs/resources/SPEC.md`
#[doc(hidden)]
pub fn str001_dependency_smoke() -> usize {
    use chia_bls::Signature;
    use chia_protocol::Bytes32;
    use chia_sha2::Sha256;
    use chia_traits::Streamable;

    fn touch_streamable<T: Streamable>(v: &T) {
        let _ = core::mem::size_of_val(v);
    }

    #[derive(serde::Serialize)]
    struct SerdeSmoke {
        tag: u8,
    }

    #[derive(thiserror::Error, Debug)]
    enum SmokeErr {
        #[error("str-001 dependency smoke")]
        Smoke,
    }

    let _: Bytes32 = Bytes32::default();
    let _epoch: u64 = BLOCKS_PER_EPOCH;
    let _net = core::mem::size_of::<NetworkConstants>();
    let _hdr = core::mem::size_of::<L2BlockHeader>();
    let _rocksdb = core::mem::size_of::<rocksdb::DB>();
    let _zstd = zstd::DEFAULT_COMPRESSION_LEVEL;
    let _codec = bincode::serialized_size(&0u8).unwrap_or(0);
    let _serde = bincode::serialize(&SerdeSmoke { tag: 0 })
        .map(|b| b.len())
        .unwrap_or(0);
    let _err = SmokeErr::Smoke.to_string().len();
    let _sig = core::mem::size_of::<Signature>();
    touch_streamable(&Bytes32::default());
    let mut sha = Sha256::new();
    sha.update(b"str001");
    let _sha_len = sha.finalize().len();
    let _lock = core::mem::size_of::<parking_lot::RawRwLock>();
    let _lru = core::mem::size_of::<lru::LruCache<u8, u8>>();
    let _rt = core::mem::size_of::<tokio::runtime::Runtime>();
    let _map = core::mem::size_of::<memmap2::MmapOptions>();

    _hdr + _net
        + _rocksdb
        + _zstd as usize
        + _codec as usize
        + _serde
        + _err
        + _sig
        + _sha_len
        + _lock
        + _lru
        + _rt
        + _map
        + _epoch as usize
}
