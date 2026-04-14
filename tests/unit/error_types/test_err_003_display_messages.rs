//! # ERR-003 — `Display` / `to_string()` quality for every `BlockStoreError` variant
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`ERR-003_error_display_messages.md`](../../../docs/requirements/domains/error_types/specs/ERR-003_error_display_messages.md) — `#[error]` contract, acceptance table, test plan
//! - [`NORMATIVE` ERR-003](../../../docs/requirements/domains/error_types/NORMATIVE.md#err-003-error-display-messages) — hashes hex-encoded, struct fields inlined, static text for unit variants
//! - [`VERIFICATION.md`](../../../docs/requirements/domains/error_types/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! [`thiserror`](https://docs.rs/thiserror) expands each `#[error("…")]` on [`dig_blockstore::BlockStoreError`] into
//! [`std::fmt::Display`]. These tests lock the **observable log surface**: operators should see hashes, heights,
//! epochs, and wrapped dependency text without spelunking `Debug` or `source()`.
//!
//! **Bytes32:** [`chia_protocol::Bytes32`] formats as lowercase hex (no `0x`) via its [`Display`](std::fmt::Display)
//! impl — ERR-003 / NORMATIVE require that hash context appear in `BlockNotFound` and `BlockNotInStore` strings.

use std::fmt::Write as _;

use chia_protocol::Bytes32;
use dig_blockstore::BlockStoreError;
use rocksdb::{Options, DB};

/// First four bytes spell `deadbeef` in hex — easy substring assertion without importing `hex`.
fn sample_hash() -> Bytes32 {
    let mut b = [0u8; 32];
    b[0] = 0xde;
    b[1] = 0xad;
    b[2] = 0xbe;
    b[3] = 0xef;
    Bytes32::new(b)
}

fn sample_rocksdb_error() -> rocksdb::Error {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_dir = tmp.path().join("not_a_directory");
    std::fs::write(&not_a_dir, b"x").expect("write file");
    DB::open(&Options::default(), &not_a_dir).expect_err("rocksdb open should fail")
}

#[test]
fn test_all_thirteen_variants_display_nonempty() {
    // **ERR-003** acceptance §1 + NORMATIVE (includes `EmptyReorgChain` / `PipelineClosed` beyond the spec table).
    let rocks_inner = sample_rocksdb_error();
    let cases = [
        BlockStoreError::RocksDb(rocks_inner),
        BlockStoreError::Serialization("payload".into()),
        BlockStoreError::Compression("zstd".into()),
        BlockStoreError::BlockNotFound(sample_hash()),
        BlockStoreError::CheckpointNotFound(42),
        BlockStoreError::BlockNotInStore(sample_hash()),
        BlockStoreError::RollbackBelowMin {
            target: 50,
            min: 100,
        },
        BlockStoreError::RollbackAboveTip {
            target: 200,
            tip: 150,
        },
        BlockStoreError::NoTip,
        BlockStoreError::SchemaMismatch {
            expected: 2,
            found: 1,
        },
        BlockStoreError::NotInitialized,
        BlockStoreError::EmptyReorgChain,
        BlockStoreError::PipelineClosed,
    ];
    assert_eq!(
        cases.len(),
        13,
        "ERR-001/ERR-003 cover exactly thirteen variants"
    );
    for (i, err) in cases.into_iter().enumerate() {
        let s = err.to_string();
        assert!(
            !s.trim().is_empty(),
            "variant index {i} produced empty Display"
        );
    }
}

#[test]
fn test_block_not_found_and_block_not_in_store_include_hex_hash() {
    // **Test plan §2** / NORMATIVE — hash context must appear (hex encoding of [`Bytes32`]).
    let h = sample_hash();
    let s1 = BlockStoreError::BlockNotFound(h).to_string();
    let s2 = BlockStoreError::BlockNotInStore(h).to_string();
    assert!(
        s1.contains("deadbeef"),
        "BlockNotFound display should include hex hash: {s1}"
    );
    assert!(
        s2.contains("deadbeef"),
        "BlockNotInStore display should include hex hash: {s2}"
    );
    assert!(
        s1.starts_with("block not found:"),
        "prefix should match #[error] template: {s1}"
    );
    assert!(
        s2.starts_with("block not in store:"),
        "prefix should match #[error] template: {s2}"
    );
}

#[test]
fn test_checkpoint_not_found_includes_epoch() {
    // **Test plan §3**
    let s = BlockStoreError::CheckpointNotFound(42).to_string();
    assert!(
        s.contains("42"),
        "epoch must appear in Display (checkpoint not found): {s}"
    );
    assert!(
        s.contains("epoch"),
        "message should name the field semantically: {s}"
    );
}

#[test]
fn test_rollback_variants_include_both_heights() {
    // **Test plan §4–5**
    let below = BlockStoreError::RollbackBelowMin {
        target: 50,
        min: 100,
    }
    .to_string();
    assert!(below.contains("50"), "{below}");
    assert!(below.contains("100"), "{below}");

    let above = BlockStoreError::RollbackAboveTip {
        target: 200,
        tip: 150,
    }
    .to_string();
    assert!(above.contains("200"), "{above}");
    assert!(above.contains("150"), "{above}");
}

#[test]
fn test_schema_mismatch_includes_versions() {
    // **Test plan §6**
    let s = BlockStoreError::SchemaMismatch {
        expected: 2,
        found: 1,
    }
    .to_string();
    assert!(s.contains('2'), "{s}");
    assert!(s.contains('1'), "{s}");
    assert!(
        s.contains("expected") && s.contains("found"),
        "labels should guide operators: {s}"
    );
}

#[test]
fn test_no_tip_and_not_initialized_exact_messages() {
    // **Test plan §7** — exact static strings avoid churn in log parsers / golden tests.
    assert_eq!(BlockStoreError::NoTip.to_string(), "no chain tip set");
    assert_eq!(
        BlockStoreError::NotInitialized.to_string(),
        "store not initialized"
    );
}

#[test]
fn test_serialization_and_compression_embed_payload() {
    // **Test plan §8–9** / acceptance §3–4
    let msg = "test error msg";
    let ser = BlockStoreError::Serialization(msg.into()).to_string();
    assert!(
        ser.contains(msg),
        "serialization error should embed caller/context string: {ser}"
    );
    assert!(
        ser.starts_with("serialization error:"),
        "prefix labels the subsystem: {ser}"
    );

    let z = "zstd failure";
    let comp = BlockStoreError::Compression(z.into()).to_string();
    assert!(comp.contains(z), "{comp}");
    assert!(
        comp.starts_with("compression error:"),
        "prefix labels the subsystem: {comp}"
    );
}

#[test]
fn test_rocksdb_display_includes_inner_text() {
    // **Acceptance §2** — wrapper must not swallow the RocksDB [`Display`](std::fmt::Display).
    let inner = sample_rocksdb_error();
    let mut inner_s = String::new();
    write!(&mut inner_s, "{inner}").unwrap();
    let wrapped = BlockStoreError::RocksDb(inner);
    let out = wrapped.to_string();
    assert!(out.contains("rocksdb error:"), "template prefix: {out}");
    assert!(
        out.contains(inner_s.trim()),
        "outer message should include inner RocksDB text: {out}"
    );
}

#[test]
fn test_empty_reorg_chain_and_pipeline_closed_are_actionable() {
    // **NORMATIVE** — descriptive text without extra fields; must still guide the caller.
    let reorg = BlockStoreError::EmptyReorgChain.to_string();
    assert!(
        reorg.contains("empty") && reorg.contains("reorg"),
        "{reorg}"
    );

    let pipe = BlockStoreError::PipelineClosed.to_string();
    assert!(
        pipe.contains("pipeline") || pipe.contains("closed"),
        "{pipe}"
    );
}
