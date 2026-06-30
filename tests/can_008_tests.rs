//! # CAN-008 — `CanonicalDenseFile` offset-overflow guards + zero-slot read semantics
//!
//! **Trace**
//! - Spec: [`CAN-002.md`](../docs/requirements/domains/canonical_chain/specs/CAN-002.md) — dense
//!   `height × 32` layout, read/write/truncate offset math.
//!
//! ## What this file proves
//!
//! [`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) computes byte offsets
//! as `height * 32` (read) and `(height + 1) * 32` (write/truncate). For heights near
//! [`u64::MAX`] those `checked_mul` / `checked_add` operations overflow `usize`, and the
//! implementation must return a [`BlockStoreError::Serialization`] "height overflow" rather than
//! panic or silently wrap. The existing CAN-002 suite only exercises small in-range heights, leaving
//! those guard branches uncovered. This file drives each overflow guard plus the
//! [`CanonicalDenseFile::read_hash`] contract that an **in-range all-zero** slot still returns
//! `Some(Bytes32::default)` (the "is a gap, but mapped" vs "beyond the tail" distinction).

#![forbid(unsafe_code)]

use chia_protocol::Bytes32;
use dig_blockstore::canonical::CanonicalDenseFile;
use dig_blockstore::BlockStoreError;

fn open_tmp() -> (tempfile::TempDir, CanonicalDenseFile) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("canonical.bin");
    let f = CanonicalDenseFile::open_read_write(&path).expect("open rw");
    (dir, f)
}

fn distinct_hash(seed: u8) -> Bytes32 {
    let mut b = [0u8; 32];
    b[0] = seed;
    b[31] = seed.wrapping_add(1);
    Bytes32::new(b)
}

/// **Proves:** CAN-002 read offset guard — `height * 32` that overflows `usize` returns a
/// `Serialization("...height overflow")` error, not a panic. `u64::MAX / 4` * 32 overflows usize on
/// 64-bit targets.
#[test]
fn test_read_hash_offset_overflow_errors() {
    let (_dir, f) = open_tmp();
    let err = f
        .read_hash(u64::MAX)
        .expect_err("u64::MAX * 32 must overflow and error");
    match err {
        BlockStoreError::Serialization(msg) => {
            assert!(
                msg.contains("overflow"),
                "expected overflow message, got {msg}"
            );
        }
        other => panic!("expected Serialization overflow, got {other:?}"),
    }
}

/// **Proves:** CAN-002 write offset guard — `(height + 1) * 32` that overflows returns a
/// `Serialization("...height overflow")` error rather than panicking on the `checked_add`/`checked_mul`.
#[test]
fn test_write_hash_offset_overflow_errors() {
    let (_dir, mut f) = open_tmp();
    let err = f
        .write_hash(u64::MAX, &distinct_hash(1))
        .expect_err("(u64::MAX + 1) * 32 must overflow and error");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("overflow")),
        "expected write overflow Serialization error, got {err:?}"
    );
}

/// **Proves:** CAN-002 truncate offset guard — `(max_height + 1) * 32` overflow is reported as a
/// `Serialization` error, covering the truncate guard branch.
#[test]
fn test_truncate_offset_overflow_errors() {
    let (_dir, mut f) = open_tmp();
    let err = f
        .truncate(u64::MAX)
        .expect_err("(u64::MAX + 1) * 32 must overflow and error");
    assert!(
        matches!(err, BlockStoreError::Serialization(ref m) if m.contains("overflow")),
        "expected truncate overflow Serialization error, got {err:?}"
    );
}

/// **Proves:** CAN-002 read contract — an in-range slot that was grown over (never written) is
/// mapped, so `read_hash` returns `Some(all-zero)` (a gap inside the dense prefix), distinct from a
/// height beyond the file which returns `None`. Writing height 5 grows the file to cover 0..=5, so
/// the skipped slot 2 is in-range-but-zero.
#[test]
fn test_read_hash_in_range_zero_slot_is_some() {
    let (_dir, mut f) = open_tmp();
    f.write_hash(5, &distinct_hash(9))
        .expect("write h5 (grows file)");
    let slot2 = f.read_hash(2).expect("read in-range slot 2");
    assert_eq!(
        slot2,
        Some(Bytes32::default()),
        "in-range never-written slot must read as Some(all-zero), not None"
    );
    // Height 6 is one past the tail → None.
    assert!(
        f.read_hash(6).expect("read past tail").is_none(),
        "height beyond the mapped tail must be None"
    );
}

/// **Proves:** CAN-002 — overwriting an existing height in place (no growth) replaces the hash and
/// does not change the file length, covering the write path's "need <= cur" no-remap branch.
#[test]
fn test_write_hash_in_place_overwrite_no_growth() {
    let (_dir, mut f) = open_tmp();
    f.write_hash(0, &distinct_hash(1)).expect("write v1");
    f.write_hash(1, &distinct_hash(2))
        .expect("write h1 grows to 2 slots");
    let len_before = f.len_bytes();
    f.write_hash(0, &distinct_hash(3))
        .expect("overwrite h0 in place");
    assert_eq!(
        f.len_bytes(),
        len_before,
        "in-place overwrite must not grow file"
    );
    assert_eq!(f.read_hash(0).expect("r0").unwrap(), distinct_hash(3));
    assert_eq!(f.read_hash(1).expect("r1").unwrap(), distinct_hash(2));
}

/// **Proves:** CAN-002 — truncating to a height at or above the current tip is a clamp that keeps the
/// file at least as long, and reads of retained heights are preserved (truncate remap path with no
/// data loss for in-range heights).
#[test]
fn test_truncate_to_current_tip_preserves_data() {
    let (_dir, mut f) = open_tmp();
    for h in 0u64..10 {
        f.write_hash(h, &distinct_hash(h as u8)).expect("write");
    }
    f.truncate(9).expect("truncate to current tip");
    assert_eq!(f.len_bytes(), 10 * 32, "truncate to tip keeps all 10 slots");
    for h in 0u64..10 {
        assert_eq!(
            f.read_hash(h).expect("read").unwrap(),
            distinct_hash(h as u8)
        );
    }
}
