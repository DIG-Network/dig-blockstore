//! # ERR-001 — `BlockStoreError` enum completeness and trait contracts
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`ERR-001_blockstoreerror_enum.md`](../../../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) — thirteen variants, `thiserror`, test plan §1–5
//! - [`NORMATIVE` ERR-001](../../../docs/requirements/domains/error_types/NORMATIVE.md#err-001-blockstoreerror-enum)
//! - [`VERIFICATION.md`](../../../docs/requirements/domains/error_types/VERIFICATION.md) — matrix row ERR-001
//!
//! ## What this file proves
//!
//! 1. **Constructibility** — Every variant listed in ERR-001 can be built with the documented inner types,
//!    so downstream modules (`store`, canonical, rollback) can return them without resorting to `unreachable!`.
//! 2. **`Debug`** — Observability in logs/tests: each arm formats to a non-empty `{:?}` string.
//! 3. **`std::error::Error::source`** — Only [`BlockStoreError::RocksDb`] chains an inner [`rocksdb::Error`];
//!    other variants are leaves (ERR-001 test plan expectation).
//! 4. **`Send + Sync`** — Required for `Result<T, BlockStoreError>` across `tokio` task boundaries ([`ERR-001`
//!    acceptance §6](../../../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md)).
//! 5. **Exhaustiveness** — A single `match` over all thirteen variants must compile, guarding against silent
//!    enum drift when new failure modes land in later requirements.

use std::error::Error;
use std::fmt::Write as _;

use chia_protocol::Bytes32;
use dig_blockstore::BlockStoreError;
use rocksdb::{Options, DB};

/// Counts which variant was matched — proves the `match` is exhaustive (compiler-checked).
fn err_discriminant(e: &BlockStoreError) -> u8 {
    match e {
        BlockStoreError::RocksDb(_) => 0,
        BlockStoreError::Serialization(_) => 1,
        BlockStoreError::Compression(_) => 2,
        BlockStoreError::BlockNotFound(_) => 3,
        BlockStoreError::CheckpointNotFound(_) => 4,
        BlockStoreError::BlockNotInStore(_) => 5,
        BlockStoreError::RollbackBelowMin { .. } => 6,
        BlockStoreError::RollbackAboveTip { .. } => 7,
        BlockStoreError::NoTip => 8,
        BlockStoreError::SchemaMismatch { .. } => 9,
        BlockStoreError::NotInitialized => 10,
        BlockStoreError::EmptyReorgChain => 11,
        BlockStoreError::PipelineClosed => 12,
    }
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn test_construct_all_variants_and_debug_non_empty() {
    let rocks = sample_rocksdb_error();
    let cases: Vec<BlockStoreError> = vec![
        BlockStoreError::RocksDb(rocks),
        BlockStoreError::Serialization("bincode".into()),
        BlockStoreError::Compression("zstd".into()),
        BlockStoreError::BlockNotFound(Bytes32::new([1u8; 32])),
        BlockStoreError::CheckpointNotFound(7),
        BlockStoreError::BlockNotInStore(Bytes32::new([2u8; 32])),
        BlockStoreError::RollbackBelowMin { target: 1, min: 2 },
        BlockStoreError::RollbackAboveTip { target: 9, tip: 3 },
        BlockStoreError::NoTip,
        BlockStoreError::SchemaMismatch {
            expected: 1,
            found: 2,
        },
        BlockStoreError::NotInitialized,
        BlockStoreError::EmptyReorgChain,
        BlockStoreError::PipelineClosed,
    ];
    assert_eq!(cases.len(), 13, "ERR-001 defines exactly thirteen variants");

    let mut buf = String::new();
    for (i, e) in cases.iter().enumerate() {
        assert_eq!(usize::from(err_discriminant(e)), i);
        buf.clear();
        write!(&mut buf, "{e:?}").expect("Debug fmt");
        assert!(
            buf.len() > 3,
            "variant {i} should produce meaningful Debug: {buf}"
        );
    }
}

#[test]
fn test_rocksdb_source_chains_inner_other_variants_are_leaves() {
    let inner = sample_rocksdb_error();
    let wrapped = BlockStoreError::RocksDb(inner);
    assert!(
        wrapped.source().is_some(),
        "RocksDb should forward source() per ERR-001 test plan"
    );

    let leaf = BlockStoreError::Serialization("x".into());
    assert!(leaf.source().is_none());
}

#[test]
fn test_from_rocksdb_error() {
    let inner = sample_rocksdb_error();
    let e: BlockStoreError = inner.into();
    assert!(matches!(e, BlockStoreError::RocksDb(_)));
}

#[test]
fn test_send_sync_bounds() {
    assert_send_sync::<BlockStoreError>();
}

/// Produce a real [`rocksdb::Error`] by opening a path that is a file, not a directory (RocksDB fails fast).
fn sample_rocksdb_error() -> rocksdb::Error {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_dir = tmp.path().join("not_a_directory");
    std::fs::write(&not_a_dir, b"x").expect("write file");
    DB::open(&Options::default(), &not_a_dir).expect_err("expected RocksDB open failure")
}
