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
//! constants, errors, encoding, cache, canonical index, compression, async pipeline,
//! and snapshot I/O.
//!
//! Crate-root **`pub use` re-exports** are deferred to [`STR-003`](../docs/requirements/domains/crate_structure/specs/STR-003.md);
//! consumers should use paths like [`store::BlockStore`](crate::store::BlockStore) until then.
//!
//! ## STR-001 dependency smoke
//!
//! [`str001_dependency_smoke`] forces every direct `Cargo.toml` dependency to link,
//! catching resolution failures early ([`STR-001`](../docs/requirements/domains/crate_structure/specs/STR-001.md)).
#![forbid(unsafe_code)]

pub mod cache;
pub mod canonical;
pub mod compression;
pub mod config;
pub mod constants;
pub mod encoding;
pub mod error;
pub mod pipeline;
pub mod snapshot;
pub mod store;
pub mod types;

use dig_block::L2BlockHeader;
use dig_constants::NetworkConstants;
use dig_epoch::DigEpochStub;

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
    let _epoch: DigEpochStub = DigEpochStub;
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
}
