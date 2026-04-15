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

/// Sidecar filename co-located with the RocksDB directory ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md)).
/// Sidecar basename next to the RocksDB directory (STR-002 / integration tests may reference this string).
pub const CANONICAL_BIN_FILE: &str = "canonical.bin";

/// Absolute path to `canonical.bin` for a store whose RocksDB lives at `db_dir`.
#[must_use]
pub(crate) fn canonical_bin_path(db_dir: &Path) -> PathBuf {
    db_dir.join(CANONICAL_BIN_FILE)
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

/// On-disk + mmap view of `canonical.bin`.
///
/// **Variants:** read-write stores use [`CanonicalBin::Rw`] so [`Self::extend_write`] can grow the file; read-only
/// stores use [`CanonicalBin::Ro`]. [`CanonicalBin::Disabled`] means every height lookup must use RocksDB only.
pub(crate) enum CanonicalBin {
    /// Writable mmap — normal [`BlockStore::open`](crate::store::BlockStore::open).
    Rw {
        /// Retained so the mapping stays valid and so we can [`File::set_len`] before remap.
        _file: File,
        mmap: MmapMut,
    },
    /// Read-only mmap — [`BlockStore::open_readonly`](crate::store::BlockStore::open_readonly).
    Ro { _file: File, mmap: Mmap },
    /// Acceleration off (tests simulating “mmap unavailable”, or readonly open when the file cannot be trusted).
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
            // No canonical rows yet — create an empty sidecar so later [`Self::extend_write`] has a path ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md) AC).
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
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|e| {
                    BlockStoreError::Serialization(format!(
                        "canonical.bin rw open {}: {e}",
                        path.display()
                    ))
                })?;
            // SAFETY: We opened this file exclusively for the store; we do not shrink it while mapped except via
            // [`Self::extend_write`], which remaps after [`File::set_len`].
            let len = file
                .metadata()
                .map_err(|e| {
                    BlockStoreError::Serialization(format!(
                        "canonical.bin metadata {}: {e}",
                        path.display()
                    ))
                })?
                .len() as usize;
            let mmap = unsafe { MmapOptions::new().len(len).map_mut(&file) }.map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "canonical.bin mmap_mut {}: {e}",
                    path.display()
                ))
            })?;
            Ok(Self::Rw { _file: file, mmap })
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

    /// Read 32 bytes at `height` when present and non-zero ([`CAN-002`](../../docs/requirements/domains/canonical_chain/specs/CAN-002.md)).
    ///
    /// **Returns:** `None` when disabled, height out of range, or the dense slot is all zeros (gap).
    pub(crate) fn read_hash_bytes(&self, height: u64) -> Option<[u8; 32]> {
        let off = usize::try_from(height.checked_mul(32)?).ok()?;
        match self {
            Self::Rw { mmap, .. } => read_slice_nonzero(mmap.as_ref(), off),
            Self::Ro { mmap, .. } => read_slice_nonzero(mmap.as_ref(), off),
            Self::Disabled => None,
        }
    }

    /// Grow the file if needed and write `hash` at `height` (must match RocksDB write that already succeeded).
    pub(crate) fn extend_write(
        &mut self,
        height: u64,
        hash: &Bytes32,
    ) -> Result<(), BlockStoreError> {
        let Self::Rw {
            ref mut mmap,
            ref mut _file,
        } = self
        else {
            return Ok(());
        };
        let need = (height as usize)
            .checked_add(1)
            .and_then(|n| n.checked_mul(32))
            .ok_or_else(|| {
                BlockStoreError::Serialization("canonical.bin extend: height overflow".into())
            })?;
        if need > mmap.len() {
            _file.set_len(need as u64).map_err(|e| {
                BlockStoreError::Serialization(format!("canonical.bin set_len: {e}"))
            })?;
            // SAFETY: File length was increased to `need`; we remap the full span.
            *mmap = unsafe { MmapOptions::new().len(need).map_mut(&*_file) }.map_err(|e| {
                BlockStoreError::Serialization(format!("canonical.bin remap after extend: {e}"))
            })?;
        }
        let o = (height as usize) * 32;
        mmap[o..o + 32].copy_from_slice(hash.as_ref());
        Ok(())
    }

    /// **Diagnostics / CAN-001 tests:** Turn off mmap reads so the store exercises RocksDB fallback.
    pub(crate) fn disable(&mut self) {
        *self = Self::Disabled;
    }
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
