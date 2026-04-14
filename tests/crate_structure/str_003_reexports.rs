//! # STR-003 — Crate-root public re-exports
//!
//! **Trace (per `docs/prompt/start.md`)**
//! - [`STR-003.md`](../../../docs/requirements/domains/crate_structure/specs/STR-003.md) — required `pub use` list and consumer `use dig_blockstore::{…}` example
//! - [`NORMATIVE.md` (STR-003)](../../../docs/requirements/domains/crate_structure/NORMATIVE.md) — same symbol set
//! - [`SPEC.md` §15](../../../docs/resources/SPEC.md) — public API layout
//!
//! ## What this file proves
//!
//! 1. **`test_all_reexports_importable_from_crate_root`** — A single `use dig_blockstore::{…}`
//!    statement pulls every type, error, column-family constant, metadata key constant, and key
//!    encoding function that STR-003 mandates. If `lib.rs` dropped a `pub use`, this test crate
//!    fails to compile, which is stronger than a string search: it is exactly the consumer
//!    experience described in the spec’s “Consumers MUST be able to write” block.
//!
//! 2. **`test_cf_constants_values` / `test_meta_constants_values`** — Matches the STR-003 test
//!    plan rows for CF/META string values, proving re-exports point at the same definitions as
//!    [`TYP-001`](../../../docs/requirements/domains/storage_types/specs/TYP-001.md) /
//!    [`TYP-002`](../../../docs/requirements/domains/storage_types/specs/TYP-002.md).
//!
//! 3. **`test_encoding_functions_round_trip_epoch`** — Exercises [`epoch_key`] /
//!    [`decode_epoch_key`] re-exported from the crate root, covering KEY-003 behavior expected
//!    of the encoding surface.

use chia_protocol::Bytes32;

use dig_blockstore::{
    decode_epoch_key, epoch_key, hash_key, height_key, metadata_key, BlockRecord, BlockStore,
    BlockStoreConfig, BlockStoreError, ChainTip, StorageStats, StoredCheckpoint, CF_ATTESTED,
    CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS, CF_METADATA, META_GENESIS_HASH,
    META_MIN_HEIGHT, META_SCHEMA_VERSION, META_TIP, META_ZSTD_DICT,
};

#[test]
fn test_all_reexports_importable_from_crate_root() {
    let _ = core::mem::size_of::<BlockStore>();
    let _ = core::mem::size_of::<BlockStoreConfig>();
    let _ = core::mem::size_of::<BlockRecord>();
    let _ = core::mem::size_of::<StoredCheckpoint>();
    let _ = core::mem::size_of::<ChainTip>();
    let _ = core::mem::size_of::<StorageStats>();
    let _ = core::mem::size_of::<BlockStoreError>();
    let _ = height_key(0);
    let _ = epoch_key(1);
    let _ = decode_epoch_key(&epoch_key(42));
    let _ = metadata_key(META_TIP);
    let h = Bytes32::default();
    assert_eq!(hash_key(&h).len(), 32);
}

#[test]
fn test_cf_constants_values() {
    assert_eq!(CF_BLOCKS, "blocks");
    assert_eq!(CF_HEADERS, "headers");
    assert_eq!(CF_ATTESTED, "attested");
    assert_eq!(CF_CANONICAL, "canonical");
    assert_eq!(CF_CHECKPOINTS, "checkpoints");
    assert_eq!(CF_METADATA, "metadata");
}

#[test]
fn test_meta_constants_values() {
    assert_eq!(META_TIP, "tip");
    assert_eq!(META_GENESIS_HASH, "genesis_hash");
    assert_eq!(META_MIN_HEIGHT, "min_height");
    assert_eq!(META_SCHEMA_VERSION, "schema_version");
    assert_eq!(META_ZSTD_DICT, "zstd_dict");
}

#[test]
fn test_encoding_functions_round_trip_epoch() {
    for e in [0u64, 1, u64::MAX / 2, u64::MAX] {
        assert_eq!(decode_epoch_key(&epoch_key(e)), e);
    }
}

#[test]
fn test_metadata_key_utf8_tip() {
    assert_eq!(metadata_key("tip"), &[0x74, 0x69, 0x70]);
}
