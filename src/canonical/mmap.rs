//! Memory-mapped `canonical.bin` — dense `height × 32` bytes of header hashes ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md), [`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md)).
//!
//! **Rationale:** [`CF_CANONICAL`](crate::constants::CF_CANONICAL) is durable but microsecond-scale; a file-backed
//! mmap gives a pointer + offset read path (~page-cache resident) for steady-state height→hash resolution before
//! loading headers/bodies ([`CAN-006`](../../docs/requirements/domains/canonical_chain/specs/CAN-006.md) will formalize the public API).
//!
//! **Recovery:** On open, bytes are validated against a dense reconstruction from RocksDB; any mismatch (missing
//! file, wrong length, wrong hash at any indexed height) triggers a full rewrite from [`CF_CANONICAL`] — the DB is
//! the source of truth ([`NORMATIVE` § CAN-001](../../docs/requirements/domains/canonical_chain/NORMATIVE.md)).
//!
//! # Safety
//!
//! [`memmap2::MmapOptions::map_mut`] / [`map`](memmap2::MmapOptions::map) are `unsafe` because the caller must
//! ensure the file is not truncated concurrently by another actor in ways that violate mapping invariants. This
//! crate owns `canonical.bin` exclusively next to the RocksDB directory; writers hold the store mutex/`RwLock` rules
//! documented on [`crate::store::BlockStoreInner`].
#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chia_protocol::Bytes32;
use memmap2::{Mmap, MmapMut, MmapOptions};
use rocksdb::{ColumnFamily, DB};

use crate::constants::CF_CANONICAL;
use crate::encoding::decode_height_key;
use crate::error::BlockStoreError;

/// Sidecar basename next to the RocksDB directory ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md); STR-002 wiring in `tests/str_002_tests.rs`).
pub const CANONICAL_BIN_FILE: &str = "canonical.bin";

/// Absolute path to `canonical.bin` for a store whose RocksDB lives at `db_dir`.
#[must_use]
pub(crate) fn canonical_bin_path(db_dir: &Path) -> PathBuf {
    db_dir.join(CANONICAL_BIN_FILE)
}

fn read_slice_nonzero(mmap: &[u8], off: usize) -> Option<[u8; 32]> {
    if off + 32 > mmap.len() {
        return None;
    }
    let s = &mmap[off..off + 32];
    if s.iter().all(|&b| b == 0) {
        return None;
    }
    s.try_into().ok()
}

/// Dense on-disk array of 32-byte hashes indexed by chain height ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md)).
///
/// **Layout:** Bytes `[h * 32, h * 32 + 32)` are the raw hash octets for height `h`. There is **no** header, magic,
/// or framing — only `(tallest_height_written + 1) × 32` bytes on disk (spec § File Layout).
///
/// **Reads:** [`Self::read_hash`] mirrors the normative CAN-002 snippet: in-range offsets always materialize a
/// [`Bytes32`] by **stack** copy from the mmap window (no `Vec` intermediary) — “zero-copy” in the sense that the hot
/// pointer path stays inside the OS page cache until the small `Bytes32` value is produced.
///
/// **Writes / growth:** [`Self::write_hash`] extends [`File::set_len`] when `(height + 1) × 32` exceeds the current map,
/// then remaps. CAN-002 implementation notes mention 1 MiB growth chunks to reduce remap frequency; we keep the file
/// **exactly** `(max_height + 1) × 32` bytes so the CAN-002 acceptance matrix (“exactly `(N) * 32`”) stays unambiguous
/// in tests — chunking can be revisited under load once [`CAN-007`](../../docs/requirements/domains/canonical_chain/specs/CAN-007.md) tip churn is measured.
///
/// **Truncation:** [`Self::truncate`] is required for future reorg / rollback paths ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) § Truncation).
pub struct CanonicalDenseFile {
    file: File,
    /// Always [`Some`] after a successful [`Self::open_read_write`] / remap except briefly during `set_len` on Windows
    /// ([`Self::truncate`], grow path in [`Self::write_hash`]) — mapping must be dropped before shrinking the file
    /// or Windows returns **1224** *user-mapped section* ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) § Truncation).
    mmap: Option<MmapMut>,
}

impl CanonicalDenseFile {
    /// Create if missing and open read/write with a [`memmap2::MmapMut`] covering the entire file.
    pub fn open_read_write(path: impl AsRef<Path>) -> Result<Self, BlockStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin create parent {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin open rw {}: {e}",
                    path.display()
                ))
            })?;
        Self::map_existing(file, &path)
    }

    fn map_existing(file: File, path_for_errors: &Path) -> Result<Self, BlockStoreError> {
        let len = file
            .metadata()
            .map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin metadata {}: {e}",
                    path_for_errors.display()
                ))
            })?
            .len() as usize;
        // SAFETY: `dig-blockstore` owns this file for the lifetime of [`CanonicalDenseFile`]; length matches disk.
        let mmap = unsafe { MmapOptions::new().len(len).map_mut(&file) }.map_err(|e| {
            BlockStoreError::Serialization(format!(
                "canonical.bin mmap_mut {}: {e}",
                path_for_errors.display()
            ))
        })?;
        Ok(Self {
            file,
            mmap: Some(mmap),
        })
    }

    fn mmap_active(&self) -> &MmapMut {
        self.mmap
            .as_ref()
            .expect("canonical.bin: mmap must be installed after open/remap")
    }

    fn mmap_active_mut(&mut self) -> &mut MmapMut {
        self.mmap
            .as_mut()
            .expect("canonical.bin: mmap must be installed after open/remap")
    }

    /// Current mapped span (equals on-disk length for this exclusive file handle).
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.mmap_active().len()
    }

    /// Read the 32-byte hash at `height` ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) § Read Operation).
    ///
    /// **Returns:** [`None`] when `height` is beyond the mapped tail; otherwise always [`Some`] (including all-zero
    /// slots — callers that treat zero as “gap” use [`Self::read_slot_nonzero_bytes`]).
    pub fn read_hash(&self, height: u64) -> Result<Option<Bytes32>, BlockStoreError> {
        let offset = (height as usize).checked_mul(32).ok_or_else(|| {
            BlockStoreError::Serialization("canonical.bin read: height overflow".into())
        })?;
        let end = offset + 32;
        if end > self.mmap_active().len() {
            return Ok(None);
        }
        let slice = &self.mmap_active()[offset..end];
        let arr: [u8; 32] = <[u8; 32]>::try_from(slice).map_err(|_| {
            BlockStoreError::Serialization("canonical.bin read: 32-byte window".into())
        })?;
        Ok(Some(Bytes32::new(arr)))
    }

    /// Same offset math as [`Self::read_hash`], but returns [`None`] for all-zero slots ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) sparse tail semantics inside a dense prefix).
    pub(crate) fn read_slot_nonzero_bytes(&self, height: u64) -> Option<[u8; 32]> {
        let off = usize::try_from(height.checked_mul(32)?).ok()?;
        read_slice_nonzero(self.mmap_active().as_ref(), off)
    }

    /// Write `hash` at `height`, growing the file when `(height + 1) × 32` exceeds the current map ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) § Write Operation).
    pub fn write_hash(&mut self, height: u64, hash: &Bytes32) -> Result<(), BlockStoreError> {
        let need = (height as usize)
            .checked_add(1)
            .and_then(|n| n.checked_mul(32))
            .ok_or_else(|| {
                BlockStoreError::Serialization("canonical.bin write: height overflow".into())
            })?;
        let cur = self.mmap_active().len();
        if need > cur {
            self.mmap.take();
            self.file.set_len(need as u64).map_err(|e| {
                BlockStoreError::Serialization(format!("canonical.bin set_len: {e}"))
            })?;
            self.mmap = Some(
                unsafe { MmapOptions::new().len(need).map_mut(&self.file) }.map_err(|e| {
                    BlockStoreError::Serialization(format!("canonical.bin remap after extend: {e}"))
                })?,
            );
        }
        let o = (height as usize) * 32;
        self.mmap_active_mut()[o..o + 32].copy_from_slice(hash.as_ref());
        Ok(())
    }

    /// Shrink the dense array so heights `0..=max_height` remain ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) § Truncation).
    pub fn truncate(&mut self, max_height: u64) -> Result<(), BlockStoreError> {
        let new_len = (max_height as usize)
            .checked_add(1)
            .and_then(|n| n.checked_mul(32))
            .ok_or_else(|| {
                BlockStoreError::Serialization("canonical.bin truncate: height overflow".into())
            })?;
        self.mmap.take();
        self.file.set_len(new_len as u64).map_err(|e| {
            BlockStoreError::Serialization(format!("canonical.bin truncate set_len: {e}"))
        })?;
        self.mmap = Some(
            unsafe { MmapOptions::new().len(new_len).map_mut(&self.file) }.map_err(|e| {
                BlockStoreError::Serialization(format!("canonical.bin truncate remap: {e}"))
            })?,
        );
        Ok(())
    }
}

/// Scan [`CF_CANONICAL`] and materialize the dense byte vector: byte range `[h*32, h*32+32)` is the hash at height `h`,
/// zero-filled for gaps below `max_height` ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) startup recovery).
pub(crate) fn dense_bytes_from_cf(db: &DB, cf: &ColumnFamily) -> Result<Vec<u8>, BlockStoreError> {
    let mut heights: Vec<(u64, Bytes32)> = Vec::new();
    let iter = db.iterator_cf(cf, rocksdb::IteratorMode::Start);
    for item in iter {
        let (k, v) = item.map_err(BlockStoreError::RocksDb)?;
        let key: [u8; 8] = k.as_ref().try_into().map_err(|_| {
            BlockStoreError::Serialization(
                "canonical.bin rebuild: CF_CANONICAL key must be exactly 8 bytes".into(),
            )
        })?;
        let height = decode_height_key(&key);
        let arr: [u8; 32] = <[u8; 32]>::try_from(v.as_ref()).map_err(|_| {
            BlockStoreError::Serialization(
                "canonical.bin rebuild: CF_CANONICAL value must be exactly 32 bytes".into(),
            )
        })?;
        heights.push((height, Bytes32::new(arr)));
    }
    if heights.is_empty() {
        return Ok(Vec::new());
    }
    let max_h = heights.iter().map(|(h, _)| *h).max().unwrap_or(0);
    let mut buf = vec![0u8; (max_h as usize + 1) * 32];
    for (h, hash) in heights {
        let o = (h as usize).saturating_mul(32);
        if o + 32 > buf.len() {
            return Err(BlockStoreError::Serialization(
                "canonical.bin rebuild: height exceeds dense buffer (internal error)".into(),
            ));
        }
        buf[o..o + 32].copy_from_slice(hash.as_ref());
    }
    Ok(buf)
}

/// On-disk + mmap view of `canonical.bin` for [`BlockStore`](crate::store::BlockStore) ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md)).
///
/// **Variants:** read-write stores wrap [`CanonicalDenseFile`]; read-only stores map with [`Mmap`] only.
pub(crate) enum CanonicalBin {
    Rw(CanonicalDenseFile),
    Ro { _file: File, mmap: Mmap },
    Disabled,
}

impl CanonicalBin {
    /// Open or rebuild `canonical.bin` from `db` and map it.
    ///
    /// * `writable == true` — mismatching or missing files are **rebuilt** from [`CF_CANONICAL`].
    /// * `writable == false` — if the file is missing or does not match CF, returns [`CanonicalBin::Disabled`] (no
    ///   writes allowed on a readonly store; lookups still succeed via RocksDB per [`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md)).
    pub(crate) fn open_synced(
        db: &Arc<DB>,
        db_dir: &Path,
        writable: bool,
    ) -> Result<Self, BlockStoreError> {
        let cf = db.cf_handle(CF_CANONICAL).ok_or_else(|| {
            BlockStoreError::Serialization(format!(
                "open_synced canonical.bin: missing column family {CF_CANONICAL}"
            ))
        })?;
        let expected = dense_bytes_from_cf(db, cf)?;
        let path = canonical_bin_path(db_dir);

        let on_disk = if path.exists() {
            std::fs::read(&path).map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin read {}: {e}",
                    path.display()
                ))
            })?
        } else {
            Vec::new()
        };

        if on_disk != expected {
            if !writable {
                return Ok(Self::Disabled);
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    BlockStoreError::Serialization(format!(
                        "canonical.bin create parent {}: {e}",
                        parent.display()
                    ))
                })?;
            }
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .read(true)
                .open(&path)
                .map_err(|e| {
                    BlockStoreError::Serialization(format!(
                        "canonical.bin open {} for rebuild: {e}",
                        path.display()
                    ))
                })?;
            f.write_all(&expected).map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin write {}: {e}",
                    path.display()
                ))
            })?;
            f.flush().map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin flush {}: {e}",
                    path.display()
                ))
            })?;
        } else if expected.is_empty() && !path.exists() {
            // No canonical rows yet — create an empty sidecar so later [`CanonicalDenseFile::write_hash`] has a path ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) AC).
            if writable {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        BlockStoreError::Serialization(format!(
                            "canonical.bin create parent {}: {e}",
                            parent.display()
                        ))
                    })?;
                }
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .map_err(|e| {
                        BlockStoreError::Serialization(format!(
                            "canonical.bin touch empty {}: {e}",
                            path.display()
                        ))
                    })?;
            } else {
                return Ok(Self::Disabled);
            }
        }

        Self::map_file(&path, writable)
    }

    fn map_file(path: &Path, writable: bool) -> Result<Self, BlockStoreError> {
        if writable {
            Ok(Self::Rw(CanonicalDenseFile::open_read_write(path)?))
        } else {
            let file = OpenOptions::new().read(true).open(path).map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin ro open {}: {e}",
                    path.display()
                ))
            })?;
            // SAFETY: File is opened read-only; mapping is read-only.
            let mmap = unsafe { MmapOptions::new().map(&file) }.map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin mmap {}: {e}",
                    path.display()
                ))
            })?;
            Ok(Self::Ro { _file: file, mmap })
        }
    }

    /// Read 32 bytes at `height` when present and non-zero ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md) + [`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) gap rules).
    pub(crate) fn read_hash_bytes(&self, height: u64) -> Option<[u8; 32]> {
        match self {
            Self::Rw(d) => d.read_slot_nonzero_bytes(height),
            Self::Ro { mmap, .. } => {
                let off = usize::try_from(height.checked_mul(32)?).ok()?;
                read_slice_nonzero(mmap.as_ref(), off)
            }
            Self::Disabled => None,
        }
    }

    /// Grow the file if needed and write `hash` at `height` (must match RocksDB write that already succeeded).
    pub(crate) fn extend_write(
        &mut self,
        height: u64,
        hash: &Bytes32,
    ) -> Result<(), BlockStoreError> {
        match self {
            Self::Rw(d) => d.write_hash(height, hash),
            _ => Ok(()),
        }
    }

    /// Truncate `canonical.bin` so that heights above `max_height` are removed.
    ///
    /// The file is resized to `(max_height + 1) * 32` bytes, and the mmap is remapped.
    /// On [`Self::Disabled`] or [`Self::Ro`], this is a no-op (no file to truncate).
    ///
    /// **Used by:** [`BlockStore::rollback_to_height`](crate::store::BlockStore) ([`ROR-001`]).
    pub(crate) fn truncate_to_height(&mut self, max_height: u64) -> Result<(), BlockStoreError> {
        match self {
            Self::Rw(d) => d.truncate(max_height),
            _ => Ok(()),
        }
    }

    /// **Diagnostics / CAN-001 tests:** Turn off mmap reads so the store exercises RocksDB fallback.
    pub(crate) fn disable(&mut self) {
        *self = Self::Disabled;
    }
}
