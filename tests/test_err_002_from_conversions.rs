//! # ERR-002 — `From` / `?` ergonomics for RocksDB, bincode, and zstd I/O
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`ERR-002_error_from_conversions.md`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md) — mappings, acceptance criteria, test plan
//! - [`NORMATIVE` ERR-002](../docs/requirements/domains/error_types/NORMATIVE.md#err-002-error-from-conversions)
//! - [`VERIFICATION.md`](../docs/requirements/domains/error_types/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! | Test plan row | Requirement |
//! |---------------|-------------|
//! | RocksDB `.into()` | [`From<rocksdb::Error>`](dig_blockstore::BlockStoreError) via thiserror `#[from]` ([`ERR-002` § RocksDB](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md#rocksdb-error-conversion)). |
//! | Bincode garbage | [`From<bincode::Error>`] → [`BlockStoreError::Serialization`](dig_blockstore::BlockStoreError::Serialization). |
//! | Zstd garbage | [`BlockStoreError::compression_from_io`](dig_blockstore::BlockStoreError::compression_from_io) → [`Compression`](dig_blockstore::BlockStoreError::Compression) without a conflicting [`From<std::io::Error>`]. |
//! | `?` smoke | Functions returning `Result<_, BlockStoreError>` compile when using `?` on each source ([`ERR-002` acceptance §4](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md#acceptance-criteria)). |
//! | Message preservation | `Serialization` / `Compression` strings retain the source [`Display`](std::fmt::Display) text ([`ERR-002` §5](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md#test-plan)). |

use std::fmt::Write as _;

use dig_blockstore::BlockStoreError;
use rocksdb::{Options, DB};

#[test]
fn test_rocksdb_error_converts_with_into() {
    let inner = sample_rocksdb_error();
    let original_msg = inner.to_string();
    let err: BlockStoreError = inner.into();
    assert!(
        matches!(err, BlockStoreError::RocksDb(_)),
        "ERR-002 requires RocksDb variant after .into()"
    );
    let mut s = String::new();
    write!(&mut s, "{err}").unwrap();
    assert!(
        s.contains(original_msg.trim()),
        "Display should surface RocksDB message: {s}"
    );
}

#[test]
fn test_bincode_error_maps_to_serialization_with_nonempty_message() {
    let bc_err = bincode::deserialize::<u64>(&[0xFF, 0xFF]).unwrap_err();
    let original = bc_err.to_string();
    assert!(
        !original.is_empty(),
        "sanity: bincode should explain failure"
    );
    let err = BlockStoreError::from(bc_err);
    match err {
        BlockStoreError::Serialization(msg) => {
            assert!(
                !msg.is_empty(),
                "ERR-002: Serialization payload must carry bincode context"
            );
            assert!(
                msg.contains(original.trim()),
                "ERR-002 acceptance §5: preserve bincode text (got {msg}, expected fragment of {original})"
            );
        }
        other => panic!("expected Serialization, got {other:?}"),
    }
}

#[test]
fn test_zstd_io_error_maps_to_compression() {
    let io_err = zstd::decode_all([0xFFu8, 0xFE, 0xFD].as_slice()).unwrap_err();
    let original = io_err.to_string();
    let err = BlockStoreError::compression_from_io(io_err);
    match err {
        BlockStoreError::Compression(msg) => {
            assert!(
                !msg.is_empty(),
                "ERR-002: Compression payload must carry zstd/io context"
            );
            assert!(
                msg.contains(original.trim()),
                "ERR-002 acceptance §5: preserve io error text (got {msg})"
            );
        }
        other => panic!("expected Compression, got {other:?}"),
    }
}

/// Each helper uses `?` once so the type checker proves [`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md) ergonomics.
mod question_mark_smoke {
    use dig_blockstore::BlockStoreError;
    use rocksdb::{Options, DB};

    pub(super) fn bincode_propagates() -> Result<u64, BlockStoreError> {
        Ok(bincode::deserialize(&[0x01u8])?)
    }

    pub(super) fn zstd_propagates() -> Result<Vec<u8>, BlockStoreError> {
        zstd::decode_all(&[0xAA, 0xBB][..]).map_err(BlockStoreError::compression_from_io)
    }

    pub(super) fn rocksdb_propagates() -> Result<(), BlockStoreError> {
        let tmp = tempfile::tempdir().map_err(|e| {
            BlockStoreError::Serialization(format!("tempdir for rocksdb smoke: {e}"))
        })?;
        let blocker = tmp.path().join("not_a_db_dir");
        std::fs::write(&blocker, b"x")
            .map_err(|e| BlockStoreError::Serialization(format!("setup rocksdb smoke: {e}")))?;
        DB::open(&Options::default(), &blocker)?;
        Ok(())
    }
}

#[test]
fn test_question_mark_propagates_each_source() {
    assert!(matches!(
        question_mark_smoke::bincode_propagates(),
        Err(BlockStoreError::Serialization(_))
    ));
    assert!(matches!(
        question_mark_smoke::zstd_propagates(),
        Err(BlockStoreError::Compression(_))
    ));
    assert!(matches!(
        question_mark_smoke::rocksdb_propagates(),
        Err(BlockStoreError::RocksDb(_))
    ));
}

/// Produce a real [`rocksdb::Error`] (path is a file, not a directory).
fn sample_rocksdb_error() -> rocksdb::Error {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_dir = tmp.path().join("not_a_directory");
    std::fs::write(&not_a_dir, b"x").expect("write file");
    DB::open(&Options::default(), &not_a_dir).expect_err("expected RocksDB open failure")
}
