//! # TYP-004 — [`dig_blockstore::BlockRecord`] and [`BlockRecord::from_header`]
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-004.md`](../docs/requirements/domains/storage_types/specs/TYP-004.md)
//! - NORMATIVE: [`NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md#typ-004-blockrecord-struct)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! Each test maps to a row in the TYP-004 test-plan table (or an acceptance bullet). We build a real
//! [`dig_block::L2BlockHeader`] via STR-005 [`test_header`] / mutations, call
//! [`dig_blockstore::BlockRecord::from_header`], and assert field-for-field equality with the header and
//! status-derived flags.
//!
//! **`in_canonical_chain`:** Current [`dig_block::BlockStatus`] has no `Canonical` variant (ATT-003). The
//! implementation uses [`BlockStatus::is_canonical`] (`false` only for `Orphaned` / `Rejected`); tests
//! below encode that contract instead of the obsolete `Canonical` / `Pending` pairing from an older spec
//! snippet ([`TYP-004.md`](../docs/requirements/domains/storage_types/specs/TYP-004.md) updated accordingly).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use chia_protocol::Bytes32;
use dig_block::{BlockStatus, L2BlockHeader};
use dig_blockstore::BlockRecord;

/// Header with non-default statistics so `from_header` field copies are non-trivially observable.
fn sample_header_for_stats() -> L2BlockHeader {
    let parent = Bytes32::new([7u8; 32]);
    let mut h = common::test_header(42, parent);
    h.timestamp = 1_700_000_000;
    h.proposer_index = 99;
    h.spend_bundle_count = 3;
    h.total_cost = 12_345;
    h.total_fees = 555;
    h.additions_count = 11;
    h.removals_count = 22;
    h.l1_height = 8_000_000;
    h.l1_hash = Bytes32::new([9u8; 32]);
    h.state_root = Bytes32::new([3u8; 32]);
    h
}

#[test]
fn test_from_header_identity_fields() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);
    assert_eq!(rec.hash, header.hash());
    assert_eq!(rec.height, header.height);
    assert_eq!(rec.epoch, header.epoch);
    assert_eq!(rec.parent_hash, header.parent_hash);
}

#[test]
fn test_from_header_validated_is_canonical_chain() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);
    assert!(rec.in_canonical_chain);
    assert_eq!(rec.status, BlockStatus::Validated);
}

#[test]
fn test_from_header_orphaned_not_canonical_chain() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Orphaned);
    assert!(!rec.in_canonical_chain);
    assert_eq!(rec.status, BlockStatus::Orphaned);
}

#[test]
fn test_from_header_rejected_not_canonical_chain() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Rejected);
    assert!(!rec.in_canonical_chain);
}

#[test]
fn test_from_header_pending_follows_is_canonical_predicate() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Pending);

    assert_eq!(rec.in_canonical_chain, BlockStatus::Pending.is_canonical());
    assert!(rec.in_canonical_chain);
}

#[test]
fn test_from_header_statistics() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::HardFinalized);
    assert_eq!(rec.timestamp, header.timestamp);
    assert_eq!(rec.proposer_index, header.proposer_index);
    assert_eq!(rec.spend_bundle_count, header.spend_bundle_count);
    assert_eq!(rec.total_cost, header.total_cost);
    assert_eq!(rec.total_fees, header.total_fees);
    assert_eq!(rec.additions_count, header.additions_count);
    assert_eq!(rec.removals_count, header.removals_count);
}

#[test]
fn test_from_header_l1_anchor() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::SoftFinalized);
    assert_eq!(rec.l1_height, header.l1_height);
    assert_eq!(rec.l1_hash, header.l1_hash);
}

#[test]
fn test_from_header_state_root() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);
    assert_eq!(rec.state_root, header.state_root);
}

#[test]
fn test_from_header_block_size_zero() {
    let mut header = sample_header_for_stats();

    header.block_size = 9_999;
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);

    assert_eq!(rec.block_size, 0);
}

#[test]
fn test_block_record_clone_eq() {
    let header = sample_header_for_stats();
    let a = BlockRecord::from_header(&header, BlockStatus::Validated);
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn test_block_record_debug() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);
    let s = format!("{rec:?}");
    assert!(!s.is_empty());
    assert!(s.contains("BlockRecord"));
}

#[test]
fn test_block_record_constructed_without_rocksdb() {
    let header = sample_header_for_stats();
    let _rec = BlockRecord::from_header(&header, BlockStatus::Pending);
}

#[test]
fn test_block_record_field_count_matches_normative() {
    let header = sample_header_for_stats();
    let rec = BlockRecord::from_header(&header, BlockStatus::Validated);

    let _ = (
        rec.hash,
        rec.height,
        rec.epoch,
        rec.parent_hash,
        rec.in_canonical_chain,
        rec.status,
        rec.timestamp,
        rec.proposer_index,
        rec.spend_bundle_count,
        rec.total_cost,
        rec.total_fees,
        rec.additions_count,
        rec.removals_count,
        rec.block_size,
        rec.l1_height,
        rec.l1_hash,
        rec.state_root,
    );
}
