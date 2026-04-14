//! # dig-blockstore
//!
//! Persistent storage for validated DIG L2 blocks: RocksDB layout, canonical
//! height→hash indexing, caching, rollback, pruning, and checkpoints — see
//! `docs/resources/SPEC.md` for the authoritative architecture.
//!
//! ## Bootstrap status (STR-001)
//!
//! This revision exists to satisfy
//! [`STR-001`](../docs/requirements/domains/crate_structure/specs/STR-001.md):
//! the crate root declares the full dependency surface (DIG + Chia + storage)
//! so downstream work (module layout in STR-002 onward) can compile against
//! real block types from [`dig-block`](https://github.com/DIG-Network/dig-block).
//!
//! The **public store API** (`BlockStore`, configuration, error types, etc.)
//! lands in later requirements (`STR-002`…`STR-005`, then domain specs). Until
//! then, this library exposes only a small, documented “dependency smoke” hook
//! proving the manifest resolves.
//!
//! ## Rationale for `dependency_smoke`
//!
//! Rust will not compile unused dependency crates. Touching representative types
//! and traits from each direct dependency forces the compiler to load every
//! crate listed in `Cargo.toml`, catching missing or mis-featured dependencies
//! immediately — matching the STR-001 acceptance criterion “`cargo check`
//! succeeds with no missing-crate errors”.
#![forbid(unsafe_code)]

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
