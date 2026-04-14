//! # TYP-007 — [`dig_blockstore::StorageStats`] aggregate metrics snapshot
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-007.md`](../docs/requirements/domains/storage_types/specs/TYP-007.md)
//! - NORMATIVE: [`NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md#typ-007-storagestats-struct)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! - **Defaults:** [`StorageStats::default`](dig_blockstore::StorageStats) must match the TYP-007 default
//!   table (all counts `0`, both optional heights `None`, `total_size_bytes == 0`). That is the shape
//!   [`BlockStore::stats`](../docs/requirements/domains/block_storage/specs/BLK-012.md) will fill in later (BLK-012).
//! - **Field surface:** All eight public fields from SPEC §3.5 / NORMATIVE are readable and writable in isolation,
//!   proving the type is a **pure data carrier** with no hidden RocksDB coupling ([`TYP-007` acceptance](../docs/requirements/domains/storage_types/specs/TYP-007.md)).
//! - **Derivations:** [`Clone`], [`Debug`], [`PartialEq`], [`Eq`] behave as expected for diagnostics structs
//!   (copy for cheap snapshots, equality for tests, `format!("{:?}")` for logging).
//!
//! **Semantic links:** populated stats are specified for [`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md);
//! this requirement only defines the **shape** of the snapshot, not how counts are computed.

#![forbid(unsafe_code)]

use dig_blockstore::StorageStats;

#[test]
fn test_default_counts_zero() {
    // **What:** Every `u64` counter in [`StorageStats::default`] is zero.
    // **Proves:** TYP-007 acceptance + test plan `test_default_counts_zero` + NORMATIVE field list
    // (`block_count` … `attested_count`).
    let s = StorageStats::default();
    assert_eq!(s.block_count, 0);
    assert_eq!(s.canonical_block_count, 0);
    assert_eq!(s.header_count, 0);
    assert_eq!(s.checkpoint_count, 0);
    assert_eq!(s.attested_count, 0);
}

#[test]
fn test_default_tip_height_none() {
    // **What:** Fresh stats have no tip height until a store reports one.
    // **Proves:** TYP-007 `test_default_tip_height_none` — empty / uninitialized snapshot semantics.
    assert!(StorageStats::default().tip_height.is_none());
}

#[test]
fn test_default_min_height_none() {
    // **What:** `min_height` stays `None` until pruning metadata exists ([`META_MIN_HEIGHT`](dig_blockstore::META_MIN_HEIGHT)).
    // **Proves:** TYP-007 `test_default_min_height_none`.
    assert!(StorageStats::default().min_height.is_none());
}

#[test]
fn test_default_size_zero() {
    // **What:** Disk estimate defaults to zero (no store query yet).
    // **Proves:** TYP-007 `test_default_size_zero`; BLK-012 will set this via RocksDB properties.
    assert_eq!(StorageStats::default().total_size_bytes, 0);
}

#[test]
fn test_set_and_read_fields() {
    // **What:** Callers can construct a fully specified value field-by-field (tests, mocks, future `stats()` impl).
    // **Proves:** TYP-007 `test_set_and_read_fields` + “individual fields can be set and read” + “pure data struct”.
    let s = StorageStats {
        block_count: 10,
        canonical_block_count: 7,
        header_count: 10,
        checkpoint_count: 3,
        attested_count: 2,
        tip_height: Some(99),
        min_height: Some(5),
        total_size_bytes: 1_048_576,
    };
    assert_eq!(s.block_count, 10);
    assert_eq!(s.canonical_block_count, 7);
    assert_eq!(s.header_count, 10);
    assert_eq!(s.checkpoint_count, 3);
    assert_eq!(s.attested_count, 2);
    assert_eq!(s.tip_height, Some(99));
    assert_eq!(s.min_height, Some(5));
    assert_eq!(s.total_size_bytes, 1_048_576);
}

#[test]
fn test_usable_without_rocksdb() {
    // **What:** No `BlockStore`, `DB`, or temp directory — only stack data.
    // **Proves:** TYP-007 acceptance “Can be used without a RocksDB instance (pure data struct)”.
    let _ = StorageStats {
        block_count: 0,
        canonical_block_count: 0,
        header_count: 0,
        checkpoint_count: 0,
        attested_count: 0,
        tip_height: None,
        min_height: None,
        total_size_bytes: 0,
    };
}

#[test]
fn test_clone_equality() {
    // **What:** [`Clone`] duplicates all fields; [`PartialEq`] sees the copy as equal.
    // **Proves:** TYP-007 `test_clone_equality` — snapshot types are often cloned for RPC responses.
    let a = StorageStats {
        block_count: 1,
        canonical_block_count: 1,
        header_count: 1,
        checkpoint_count: 0,
        attested_count: 0,
        tip_height: Some(0),
        min_height: None,
        total_size_bytes: 42,
    };
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn test_debug_format() {
    // **What:** [`Debug`] output is well-formed for logging.
    // **Proves:** TYP-007 `test_debug_format` — `format!("{:?}")` must not panic; we also assert stable substrings
    // so a regression that removes `Debug` or renames fields loudly fails.
    let s = StorageStats {
        block_count: 2,
        canonical_block_count: 1,
        header_count: 2,
        checkpoint_count: 1,
        attested_count: 0,
        tip_height: Some(3),
        min_height: Some(1),
        total_size_bytes: 100,
    };
    let dbg = format!("{s:?}");
    assert!(
        dbg.contains("block_count") && dbg.contains("tip_height"),
        "unexpected Debug: {dbg}"
    );
}
