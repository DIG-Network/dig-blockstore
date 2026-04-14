//! # TYP-001 — RocksDB column family name constants (`CF_*`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`TYP-001.md`](../../docs/requirements/domains/storage_types/specs/TYP-001.md) — six `pub const &str`, string values, acceptance criteria
//! - [`NORMATIVE` storage_types](../../docs/requirements/domains/storage_types/NORMATIVE.md) — CF partition contract
//! - [`SPEC.md` §2.1](../../docs/resources/SPEC.md) — authoritative storage layout reference cited by TYP-001
//!
//! ## What this file proves
//!
//! These integration tests mirror the TYP-001 test-plan table row-for-row. They import symbols from the
//! **crate root** ([`STR-003`](../../docs/requirements/domains/crate_structure/specs/STR-003.md)) so we verify
//! the same names consumers use (`use dig_blockstore::CF_BLOCKS`). Exact string equality guards against
//! accidental renames that would **orphan on-disk databases** (TYP-001: names MUST NOT change post-deploy).
//!
//! [`ALL_COLUMN_FAMILIES`](dig_blockstore::constants::ALL_COLUMN_FAMILIES) is asserted to be the complete,
//! duplicate-free set used by [`BlockStore::open`](dig_blockstore::BlockStore::open) when registering CFs.

use std::collections::HashSet;

use dig_blockstore::constants::ALL_COLUMN_FAMILIES;
use dig_blockstore::{
    BlockStore, CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS, CF_METADATA,
};

#[test]
fn test_cf_blocks_value() {
    assert_eq!(CF_BLOCKS, "blocks");
}

#[test]
fn test_cf_headers_value() {
    assert_eq!(CF_HEADERS, "headers");
}

#[test]
fn test_cf_attested_value() {
    assert_eq!(CF_ATTESTED, "attested");
}

#[test]
fn test_cf_canonical_value() {
    assert_eq!(CF_CANONICAL, "canonical");
}

#[test]
fn test_cf_checkpoints_value() {
    assert_eq!(CF_CHECKPOINTS, "checkpoints");
}

#[test]
fn test_cf_metadata_value() {
    assert_eq!(CF_METADATA, "metadata");
}

#[test]
fn test_cf_constants_distinct() {
    let set: HashSet<&str> = [
        CF_BLOCKS,
        CF_HEADERS,
        CF_ATTESTED,
        CF_CANONICAL,
        CF_CHECKPOINTS,
        CF_METADATA,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        set.len(),
        6,
        "each CF name must be unique — collisions would corrupt RocksDB routing"
    );
}

#[test]
fn test_all_column_families_is_exactly_typ001_set() {
    assert_eq!(
        ALL_COLUMN_FAMILIES.len(),
        6,
        "BlockStore must open exactly the six TYP-001 families"
    );
    let from_slice: HashSet<&str> = ALL_COLUMN_FAMILIES.iter().copied().collect();
    let expected: HashSet<&str> = [
        CF_BLOCKS,
        CF_HEADERS,
        CF_ATTESTED,
        CF_CANONICAL,
        CF_CHECKPOINTS,
        CF_METADATA,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        from_slice, expected,
        "ALL_COLUMN_FAMILIES must match the six exported CF_* constants (no drift)"
    );
}

#[test]
fn test_cf_constants_are_public_at_crate_root() {
    // Compile-time proof: `use dig_blockstore::CF_*` above; runtime smoke that store still opens with these names.
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = dig_blockstore::BlockStoreConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let _store = BlockStore::open(cfg).expect("open uses ALL_COLUMN_FAMILIES / same strings");
}
