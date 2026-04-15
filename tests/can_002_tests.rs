//! # CAN-002 — `canonical.bin` memory-mapped dense hash array
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`CAN-002.md`](../docs/requirements/domains/canonical_chain/specs/CAN-002.md)
//! - NORMATIVE: [`NORMATIVE.md` (CAN-002)](../docs/requirements/domains/canonical_chain/NORMATIVE.md#can-002-canonicalbin-memory-mapped-file)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/canonical_chain/VERIFICATION.md)
//!
//! ## What this file proves
//!
//! [`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) mandates a **flat** dense file: no
//! headers or framing, hash at height `h` at byte offset `h * 32`, total length `(max_height + 1) × 32`, growth when
//! writing past the end, truncation to a shorter tip, and backing by [`memmap2`](https://docs.rs/memmap2) for
//! page-cache-backed reads. These tests exercise the dedicated [`CanonicalDenseFile`](dig_blockstore::canonical::mmap::CanonicalDenseFile)
//! type so layout and mmap semantics are verified **without** going through full [`BlockStore`](dig_blockstore::BlockStore)
//! (integration with the store remains covered by [`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) tests).
//!
//! **Zero-copy (AC):** We assert [`CanonicalDenseFile::read_hash`] returns [`Bytes32`](chia_protocol::Bytes32) built
//! from a stack `[u8; 32]` copied out of the mmap window — no `Vec` allocation on the read path (same contract as the
//! normative snippet in CAN-002.md § Read Operation).

#![forbid(unsafe_code)]

use chia_protocol::Bytes32;
use dig_blockstore::canonical::CanonicalDenseFile;

fn distinct_hash(seed: u8) -> Bytes32 {
    let mut b = [0u8; 32];
    b[0] = seed;
    b[31] = seed.wrapping_neg();
    Bytes32::new(b)
}

/// **Proves:** CAN-002 test plan `test_file_size_matches_height` + AC “file size … `(max_height + 1) * 32`” — after
/// writing 100 distinct heights `0..=99`, the mapped length is `100 × 32 = 3200` bytes (no header overhead).
#[test]
fn test_file_size_matches_height() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    for h in 0u64..100 {
        f.write_hash(h, &distinct_hash(h as u8)).expect("write");
    }
    assert_eq!(f.len_bytes(), 100 * 32);
}

/// **Proves:** CAN-002 test plan `test_read_write_roundtrip` + AC “reading height `h` returns the 32 bytes at offset `h*32`”.
#[test]
fn test_read_write_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    let h = 42u64;
    let want = distinct_hash(7);
    f.write_hash(h, &want).expect("write");
    let got = f.read_hash(h).expect("read").expect("some");
    assert_eq!(got, want);
}

/// **Proves:** CAN-002 test plan `test_read_beyond_file_returns_none` + AC read beyond mapped span ⇒ `None`.
#[test]
fn test_read_beyond_file_returns_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    f.write_hash(0, &distinct_hash(1)).expect("write");
    assert!(f.read_hash(1).expect("read").is_none());
}

/// **Proves:** CAN-002 test plan `test_file_growth_on_write` + AC “writing … beyond current file size triggers file growth”.
#[test]
fn test_file_growth_on_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    f.write_hash(0, &distinct_hash(9)).expect("write h0");
    assert_eq!(f.len_bytes(), 32);
    f.write_hash(1000, &distinct_hash(3))
        .expect("write distant height");
    assert_eq!(f.len_bytes(), (1000u64 + 1) as usize * 32);
    assert_eq!(f.read_hash(1000).expect("r").unwrap(), distinct_hash(3));
}

/// **Proves:** CAN-002 test plan `test_truncation` + AC “truncation reduces file size to `(target_height + 1) * 32`”.
///
/// **Windows:** [`CanonicalDenseFile::truncate`](dig_blockstore::canonical::CanonicalDenseFile::truncate) drops the
/// live `MmapMut` before [`std::fs::File::set_len`]; otherwise `set_len` fails with **1224** (*user-mapped section*).
#[test]
fn test_truncation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    for h in 0u64..100 {
        f.write_hash(h, &distinct_hash(h as u8)).expect("write");
    }
    f.truncate(50).expect("truncate");
    assert_eq!(f.len_bytes(), (50u64 + 1) as usize * 32, "51 slots × 32");
    assert_eq!(f.len_bytes(), 1632);
    assert!(f.read_hash(51).expect("read").is_none());
    assert_eq!(f.read_hash(50).expect("read").unwrap(), distinct_hash(50));
}

/// **Proves:** CAN-002 test plan `test_dense_array_no_gaps` + AC dense layout for sequential heights `0,1,2`.
#[test]
fn test_dense_array_no_gaps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let mut f = CanonicalDenseFile::open_read_write(&path).expect("open");
    let a = distinct_hash(11);
    let b = distinct_hash(22);
    let c = distinct_hash(33);
    f.write_hash(0, &a).expect("h0");
    f.write_hash(1, &b).expect("h1");
    f.write_hash(2, &c).expect("h2");
    assert_eq!(f.len_bytes(), 96);
    assert_eq!(f.read_hash(0).expect("r0").unwrap(), a);
    assert_eq!(f.read_hash(1).expect("r1").unwrap(), b);
    assert_eq!(f.read_hash(2).expect("r2").unwrap(), c);
}
