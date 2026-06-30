//! # STR-006 — Crate-root dependency-smoke + wire re-export execution coverage
//!
//! **Trace**
//! - [`STR-001.md`](../docs/requirements/domains/crate_structure/specs/STR-001.md) — every direct
//!   dependency must link; the crate exposes [`dig_blockstore::str001_dependency_smoke`] as the
//!   executable proof that each `[dependencies]` entry resolves and is usable at runtime.
//! - [`STR-003.md`](../docs/requirements/domains/crate_structure/specs/STR-003.md) — flat public API.
//!
//! ## What this file proves
//!
//! The existing STR-001 suite asserts the manifest's *declared* dependency set and that `cargo check`
//! succeeds, but it never **runs** [`str001_dependency_smoke`] — so the function (which actually
//! touches every dependency: rocksdb, zstd, bincode, serde, chia-bls, chia-sha2, chia-traits,
//! parking_lot, lru, tokio, memmap2, dig-block, dig-epoch, dig-constants) was uncovered. Calling it
//! here is the runtime complement to the static manifest checks: if any direct dependency were
//! dropped or became unusable, this test would fail to compile or panic, not merely the manifest
//! parser. We also drive the [`wire`](dig_blockstore::wire) round-trip re-exports so the public
//! `block_to_wire_bytes` / `block_from_wire_bytes` surface is exercised from a downstream crate.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use dig_blockstore::{block_from_wire_bytes, block_to_wire_bytes};

use common::test_block;

/// **Proves:** STR-001 — the dependency-smoke function executes and returns a positive aggregate of
/// `size_of` probes over every direct dependency type. A zero or panic would indicate a dependency
/// failed to link or a probed type vanished.
#[test]
fn test_dependency_smoke_runs_and_is_nonzero() {
    let total = dig_blockstore::str001_dependency_smoke();
    assert!(
        total > 0,
        "str001_dependency_smoke must aggregate non-zero size_of probes across all deps"
    );
}

/// **Proves:** STR-001 — the smoke aggregate is deterministic within a single build (it is a pure
/// sum of compile-time `size_of` values plus constant lengths), guarding against accidental
/// non-determinism creeping into the dependency-probe helper.
#[test]
fn test_dependency_smoke_is_deterministic() {
    let a = dig_blockstore::str001_dependency_smoke();
    let b = dig_blockstore::str001_dependency_smoke();
    assert_eq!(a, b, "pure size_of aggregate must be stable across calls");
}

/// **Proves:** SER-003 / STR-003 — the crate-root `block_to_wire_bytes` + `block_from_wire_bytes`
/// re-exports round-trip an [`dig_block::L2Block`] byte-identically (hash preserved), exercising the
/// public wire surface that consumers integrate against.
#[test]
fn test_wire_reexports_round_trip() {
    let block = test_block(7, chia_protocol::Bytes32::default());
    let bytes = block_to_wire_bytes(&block).expect("encode to wire bytes");
    assert!(!bytes.is_empty(), "wire encoding must produce bytes");
    let decoded = block_from_wire_bytes(&bytes).expect("decode from wire bytes");
    assert_eq!(
        decoded.hash(),
        block.hash(),
        "wire round-trip must preserve block identity hash"
    );
}

/// **Proves:** SER-003 — `block_from_wire_bytes` maps a malformed/truncated frame to
/// [`BlockStoreError::Serialization`] (never a panic, never `Compression`), covering the decoder's
/// error arm.
#[test]
fn test_wire_decode_rejects_malformed_bytes() {
    let err = block_from_wire_bytes(&[0x00, 0x01, 0x02]).expect_err("malformed frame must error");
    assert!(
        matches!(err, dig_blockstore::BlockStoreError::Serialization(_)),
        "wire decode failure must surface as a Serialization error, got {err:?}"
    );
}
