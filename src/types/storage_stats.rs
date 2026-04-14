//! [`StorageStats`] — aggregate counts and disk footprint for operators / RPC ([`TYP-007`](../../docs/requirements/domains/storage_types/specs/TYP-007.md)).
//!
//! ## Requirements trace
//!
//! - **Spec:** [`TYP-007.md`](../../docs/requirements/domains/storage_types/specs/TYP-007.md) (fields, defaults, test plan)
//! - **NORMATIVE:** [`storage_types/NORMATIVE.md`](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-007-storagestats-struct)
//! - **SPEC (narrative):** [`SPEC.md`](../../docs/resources/SPEC.md) Section 3.5
//!
//! ## Rationale
//!
//! - **Pure snapshot:** This type intentionally carries **no** `RocksDB` handle or async context. [`Default`]
//!   represents an “empty / unknown” report; future `BlockStore::stats` ([`BLK-012`](../../docs/requirements/domains/block_storage/specs/BLK-012.md))
//!   will populate fields by scanning column families and metadata keys such as [`crate::constants::META_TIP`]
//!   and [`crate::constants::META_MIN_HEIGHT`].
//! - **Fork-inclusive counts:** `block_count` includes non-canonical blocks (forks), while `canonical_block_count`
//!   tracks only the current main chain — matching monitoring expectations in SPEC §3.5 wording.
//! - **`total_size_bytes`:** Rough on-disk live data size (implementation detail left to BLK-012; often
//!   `rocksdb.estimate-live-data-size` or similar), not necessarily exact filesystem usage.
//! - **`PartialEq` / `Eq`:** Not required by NORMATIVE but required by [`TYP-007`](../../docs/requirements/domains/storage_types/specs/TYP-007.md)
//!   and needed for deterministic tests and snapshot comparisons in higher layers.

/// Aggregate storage statistics for monitoring and diagnostics.
///
/// **Construction:** Prefer [`StorageStats::default`] for empty reports, then assign fields, or use a future
/// `BlockStore::stats` ([`BLK-012`](../../docs/requirements/domains/block_storage/specs/BLK-012.md)).
///
/// **Threading:** Values are typically produced on a single thread and cloned into responses; interior mutability
/// is unnecessary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageStats {
    /// Total blocks stored across all forks (CF_BLOCKS row count semantics once BLK helpers exist).
    pub block_count: u64,
    /// Blocks on the current canonical chain only.
    pub canonical_block_count: u64,
    /// Header rows (CF_HEADERS).
    pub header_count: u64,
    /// Checkpoint rows (CF_CHECKPOINTS).
    pub checkpoint_count: u64,
    /// Attested-block rows (CF_ATTESTED).
    pub attested_count: u64,
    /// Current chain tip height when known; [`None`] if the store has no tip (e.g. before genesis).
    pub tip_height: Option<u64>,
    /// Minimum retained height after pruning, from metadata; [`None`] if pruning has not run.
    pub min_height: Option<u64>,
    /// Approximate live data size on disk in bytes (see BLK-012 for how this is estimated).
    pub total_size_bytes: u64,
}
