//! # STR-002 — Module hierarchy matches SPEC §16 layout
//!
//! **Normative / spec trace (per `docs/prompt/start.md`)**
//! - [`STR-002.md`](../docs/requirements/domains/crate_structure/specs/STR-002.md) — file tree + acceptance criteria
//! - [`NORMATIVE.md` (STR-002)](../docs/requirements/domains/crate_structure/NORMATIVE.md) — same hierarchy as authoritative bullets
//! - [`SPEC.md` §16 — Crate boundary](../docs/resources/SPEC.md) — dependency direction and concerns owned by this crate
//!
//! ## What this file proves
//!
//! 1. **`test_all_required_source_files_exist`** — For every path listed in STR-002’s
//!    specification tree (`src/store.rs`, `src/types/block_record.rs`, etc.), the file
//!    exists on disk under `CARGO_MANIFEST_DIR`. This directly satisfies the STR-002
//!    test plan row “Verify each listed source file exists on disk” and mirrors the
//!    acceptance checklist (“`src/store.rs` exists and defines `BlockStore`”, …).
//!
//! 2. **`test_each_module_defines_required_public_items`** — Imports the symbols named
//!    in STR-002 (`BlockStore`, `BlockStoreConfig`, `BlockRecord`, `ShardedBlockCache`,
//!    …) from their designated modules. If a file were renamed or a type omitted, this
//!    integration test would fail to compile, proving the **module tree is wired** in
//!    `lib.rs` and each submodule exports its contractually named type.
//!
//! 3. **`test_module_tree_cargo_check_succeeds`** — Runs `cargo check` against this
//!    manifest. That matches the STR-002 test plan “`cargo check` succeeds with no
//!    missing-module errors” and guards against orphan `mod` declarations.
//!
//! ## Design notes
//!
//! - STR-003 will add **crate-root re-exports**; here we only assert **module paths**
//!   (`dig_blockstore::store::BlockStore`, etc.) as allowed by STR-002’s scope.

use std::path::PathBuf;
use std::process::Command;

use dig_blockstore::cache::sharded::ShardedBlockCache;
use dig_blockstore::cache::warming::CacheWarming;
use dig_blockstore::canonical::index::CanonicalIndex;
use dig_blockstore::canonical::mmap::CanonicalMmap;
use dig_blockstore::cf_options;
use dig_blockstore::compression::CompressionPipeline;
use dig_blockstore::config::BlockStoreConfig;
use dig_blockstore::constants::{
    CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS, CF_METADATA,
    META_GENESIS_HASH, META_MIN_HEIGHT, META_SCHEMA_VERSION, META_TIP, META_ZSTD_DICT,
};
use dig_blockstore::encoding;
use dig_blockstore::error::BlockStoreError;
use dig_blockstore::pipeline::BlockWritePipeline;
use dig_blockstore::snapshot::SnapshotIo;
use dig_blockstore::store::BlockStore;
use dig_blockstore::types::{BlockRecord, ChainTip, StorageStats, StoredCheckpoint};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn src_dir() -> PathBuf {
    manifest_dir().join("src")
}

/// Paths **relative to `src/`** mandated by
/// `docs/requirements/domains/crate_structure/specs/STR-002.md` (specification tree).
const REQUIRED_REL_PATHS: &[&str] = &[
    "lib.rs",
    "store.rs",
    "config.rs",
    "constants.rs",
    "cf_options.rs",
    "error.rs",
    "encoding.rs",
    "compression.rs",
    "pipeline.rs",
    "snapshot.rs",
    "types/mod.rs",
    "types/block_record.rs",
    "types/stored_checkpoint.rs",
    "types/chain_tip.rs",
    "types/storage_stats.rs",
    "cache/mod.rs",
    "cache/sharded.rs",
    "cache/warming.rs",
    "canonical/mod.rs",
    "canonical/index.rs",
    "canonical/mmap.rs",
];

#[test]
fn test_all_required_source_files_exist() {
    let root = src_dir();
    for rel in REQUIRED_REL_PATHS {
        let path = root.join(rel);
        assert!(
            path.is_file(),
            "STR-002 requires source file {} (resolved to {})",
            rel,
            path.display()
        );
    }
}

#[test]
fn test_each_module_defines_required_public_items() {
    // `use` lines above must compile — proves `lib.rs` declares modules and types exist.
    let _ = core::mem::size_of::<BlockStore>();
    let _ = core::mem::size_of::<BlockStoreConfig>();
    let _ = core::mem::size_of::<BlockRecord>();
    let _ = core::mem::size_of::<StoredCheckpoint>();
    let _ = core::mem::size_of::<ChainTip>();
    let _ = core::mem::size_of::<StorageStats>();
    let _ = core::mem::size_of::<BlockStoreError>();
    let _ = core::mem::size_of::<ShardedBlockCache>();
    let _ = core::mem::size_of::<CacheWarming>();
    let _ = core::mem::size_of::<CanonicalIndex>();
    let _ = core::mem::size_of::<CanonicalMmap>();
    let _ = core::mem::size_of::<CompressionPipeline>();
    let _ = cf_options::column_family_descriptors(&BlockStoreConfig::default()).len();
    let _ = core::mem::size_of::<BlockWritePipeline>();
    let _ = core::mem::size_of::<SnapshotIo>();

    assert_eq!(CF_BLOCKS, "blocks");
    assert_eq!(CF_HEADERS, "headers");
    assert_eq!(CF_ATTESTED, "attested");
    assert_eq!(CF_CANONICAL, "canonical");
    assert_eq!(CF_CHECKPOINTS, "checkpoints");
    assert_eq!(CF_METADATA, "metadata");
    assert_eq!(META_TIP, "tip");
    assert_eq!(META_GENESIS_HASH, "genesis_hash");
    assert_eq!(META_MIN_HEIGHT, "min_height");
    assert_eq!(META_SCHEMA_VERSION, "schema_version");
    assert_eq!(META_ZSTD_DICT, "zstd_dict");

    let _ = encoding::height_key(0);
}

#[test]
fn test_module_tree_cargo_check_succeeds() {
    let manifest = manifest_dir().join("Cargo.toml");
    let status = Command::new("cargo")
        .args(["check", "-q", "--manifest-path"])
        .arg(&manifest)
        .status()
        .expect("spawn cargo check");
    assert!(
        status.success(),
        "STR-002 requires full module tree to compile (cargo check)"
    );
}

/// Ensures the on-disk layout stays aligned with the **directory names** in STR-002
/// (e.g. `types/` not `typ/`).
#[test]
fn test_str002_directory_names() {
    let s = src_dir();
    for dir in ["types", "cache", "canonical"] {
        let p = s.join(dir);
        assert!(p.is_dir(), "STR-002 expects directory {}", p.display());
    }
}
