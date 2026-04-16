//! Zstd compression and bincode serialization for block bodies and headers.
//!
//! This module owns all serialization/deserialization logic:
//! - [`BlockStore::serialize_block`] / [`BlockStore::deserialize_block`]: bincode + zstd (with optional dictionary).
//! - [`BlockStore::serialize_header`] / [`BlockStore::deserialize_header`]: bincode only (no compression).
//! - Dictionary training: [`BlockStore::train_dictionary`], [`BlockStore::maybe_train_dictionary`].
//! - Dictionary loading: [`resolve_zstd_dictionary`], [`load_zstd_dict_from_db`].
//!
//! # Requirements
//!
//! - [`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) — block serialization.
//! - [`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md) — header serialization.
//! - [`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) — dictionary training.

use std::sync::Arc;

use dig_block::{L2Block, L2BlockHeader};
use rand::seq::SliceRandom;
use rocksdb::{IteratorMode, DB};

use crate::constants::{
    CF_BLOCKS, CF_METADATA, DICT_TARGET_SIZE, DICT_TRAINING_THRESHOLD, META_ZSTD_DICT,
};
use crate::error::BlockStoreError;
use crate::store::BlockStore;

/// Compression and serialization methods on [`BlockStore`].
///
/// These are `impl BlockStore` (not `BlockStoreInner`) because they were originally
/// defined in the `impl BlockStore` block and callers use `Self::serialize_header()`
/// which resolves through `BlockStore`. Field access goes through `Deref<Target=BlockStoreInner>`.
impl BlockStore {
    pub fn serialize_header(header: &L2BlockHeader) -> Result<Vec<u8>, BlockStoreError> {
        bincode::serialize(header).map_err(|e| BlockStoreError::Serialization(e.to_string()))
    }

    /// Deserialize a header from [`CF_HEADERS`] bytes ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
    ///
    /// **Read path:** raw bincode only — callers MUST NOT pass zstd-compressed payloads (those belong in [`CF_BLOCKS`]
    /// via [`Self::deserialize_block`]).
    pub fn deserialize_header(bytes: &[u8]) -> Result<L2BlockHeader, BlockStoreError> {
        bincode::deserialize(bytes).map_err(|e| BlockStoreError::Serialization(e.to_string()))
    }

    /// Serialize then zstd-compress a block for [`CF_BLOCKS`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
    ///
    /// **Pipeline:** [`bincode::serialize`] → [`zstd::bulk::Compressor::with_dictionary`] when
    /// [`Self::use_compression_dict`] and a dictionary are present; otherwise [`zstd::encode_all`] (plain zstd).
    ///
    /// **Errors:** [`BlockStoreError::Serialization`] from bincode; [`BlockStoreError::Compression`] from zstd.
    pub fn serialize_block(&self, block: &L2Block) -> Result<Vec<u8>, BlockStoreError> {
        let raw = bincode::serialize(block)?;
        if self.use_compression_dict {
            let dict_guard = self.zstd_dict.read();
            if let Some(dict) = dict_guard.as_ref() {
                let mut compressor = zstd::bulk::Compressor::with_dictionary(
                    self.compression_level,
                    dict.as_slice(),
                )
                .map_err(|e| BlockStoreError::Compression(e.to_string()))?;
                return compressor
                    .compress(raw.as_slice())
                    .map_err(BlockStoreError::compression_from_io);
            }
        }
        zstd::encode_all(raw.as_slice(), self.compression_level)
            .map_err(BlockStoreError::compression_from_io)
    }

    /// Reverse [`Self::serialize_block`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
    ///
    /// **Fallback:** Dictionary decompress is attempted first when configured; on failure, plain
    /// [`zstd::decode_all`] handles **pre-dictionary** payloads written before training ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).
    ///
    /// **Hash invariance:** Correct payloads MUST yield an [`L2Block`] whose [`L2Block::hash`] matches the original
    /// pre-serialize block ([`SER-004`](../docs/requirements/domains/serialization/specs/SER-004.md); verified in `tests/ser_004_tests.rs`).
    ///
    /// **Errors:** Decompression failures map to [`BlockStoreError::Serialization`] so callers see a single
    /// “payload unusable” surface for malformed CF_BYTES; bincode structural errors also use [`Serialization`](BlockStoreError::Serialization).
    pub fn deserialize_block(&self, compressed: &[u8]) -> Result<L2Block, BlockStoreError> {
        let raw = self.decompress_block_payload(compressed).map_err(|e| {
            BlockStoreError::Serialization(format!("deserialize_block: decompress failed: {e}"))
        })?;
        bincode::deserialize(&raw).map_err(|e| BlockStoreError::Serialization(e.to_string()))
    }

    /// Decompress a raw zstd frame from [`CF_BLOCKS`] back to bincode bytes.
    ///
    /// # Fallback strategy ([`SER-005`])
    ///
    /// When dictionary mode is active, this method tries dictionary decompression first.
    /// If that fails (because the payload was written *before* the dictionary was trained),
    /// it falls back to plain [`zstd::decode_all`]. This two-phase approach ensures all
    /// historical blocks remain readable after dictionary training—a critical invariant
    /// since DIG does not re-encode existing blocks when a dictionary is installed.
    ///
    /// # Decompression bomb protection
    ///
    /// [`zstd::bulk::Decompressor::decompress`] accepts `max_decompressed_block_bytes` as
    /// an upper bound on output size, preventing malicious payloads from exhausting memory.
    /// The plain fallback path ([`zstd::decode_all`]) does not have this cap; future work
    /// may wrap it similarly.
    pub(crate) fn decompress_block_payload(&self, compressed: &[u8]) -> std::io::Result<Vec<u8>> {
        if self.use_compression_dict {
            if let Some(dict) = self.zstd_dict.read().as_ref() {
                // Phase 1: attempt dictionary-aware decompression (post-training payloads).
                let mut decompressor = zstd::bulk::Decompressor::with_dictionary(dict.as_slice())?;
                return match decompressor.decompress(compressed, self.max_decompressed_block_bytes)
                {
                    Ok(bytes) => Ok(bytes),
                    // Phase 2: dictionary decompression failed—payload is likely a pre-training
                    // plain zstd frame. Fall back to standard decoding.
                    Err(_) => zstd::decode_all(compressed),
                };
            }
        }
        // No dictionary configured or available: standard zstd decompression.
        zstd::decode_all(compressed)
    }

    /// Retrieve a full block by hash ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)).
    ///
    /// **Order:** [`Self::block_cache`] (sharded LRU, [`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md))
    /// → on miss, `get_cf` [`CF_BLOCKS`] → [`Self::deserialize_block`] (dictionary zstd with plain fallback per [`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)).

    pub fn block_count(&self) -> Result<u64, BlockStoreError> {
        let cf = self.cf(CF_BLOCKS)?;
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        let mut n = 0u64;
        for item in iter {
            let (_k, _v) = item?;
            n = n.saturating_add(1);
        }
        Ok(n)
    }

    /// **[`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md)** — Reload dictionary bytes from
    /// [`META_ZSTD_DICT`] into memory after external maintenance (or to align with [`Self::train_dictionary`]
    /// persistence).
    ///
    /// **Startup:** [`Self::open`] already embeds this via [`load_zstd_dict_from_db`]; public callers use
    /// `init_dictionary` when a **second process** trains the dictionary or metadata is repaired online.
    pub fn init_dictionary(&self) -> Result<(), BlockStoreError> {
        let loaded = load_zstd_dict_from_db(&self.db, self.use_compression_dict)?;
        *self.zstd_dict.write() = loaded;
        Ok(())
    }

    /// Collect `sample_count` **uncompressed** bincode block bodies for [`zstd::dict::from_samples`].
    ///
    /// **Randomness:** Keys/values are shuffled with [`rand::thread_rng`] so training sees a representative slice of
    /// the corpus, not a height-ordered prefix ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) implementation notes).
    pub(crate) fn sample_block_bodies(
        &self,
        sample_count: usize,
    ) -> Result<Vec<Vec<u8>>, BlockStoreError> {
        let cf = self.cf(CF_BLOCKS)?;
        let mut blobs: Vec<Vec<u8>> = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);
        for item in iter {
            let (_key, value) = item?;
            blobs.push(value.to_vec());
        }
        if blobs.len() < sample_count {
            return Err(BlockStoreError::Serialization(format!(
                "dictionary training: need at least {sample_count} blocks in {CF_BLOCKS}, have {}",
                blobs.len()
            )));
        }
        blobs.shuffle(&mut rand::thread_rng());
        blobs.truncate(sample_count);
        let mut samples = Vec::with_capacity(sample_count);
        for compressed in blobs {
            let raw = self.decompress_block_payload(&compressed).map_err(|e| {
                BlockStoreError::Serialization(format!(
                    "dictionary training sample decompress: {e}"
                ))
            })?;
            samples.push(raw);
        }
        Ok(samples)
    }

    /// Train + persist a zstd dictionary; **idempotent** if [`META_ZSTD_DICT`] already contains bytes.
    pub(crate) fn train_dictionary(&self) -> Result<Vec<u8>, BlockStoreError> {
        let meta = self.cf(CF_METADATA)?;
        if let Some(blob) = self.db.get_cf(meta, META_ZSTD_DICT.as_bytes())? {
            if !blob.is_empty() {
                return Ok(blob);
            }
        }
        let n = DICT_TRAINING_THRESHOLD as usize;
        let samples = self.sample_block_bodies(n)?;
        let refs: Vec<&[u8]> = samples.iter().map(Vec::as_slice).collect();
        let dict = zstd::dict::from_samples(&refs, DICT_TARGET_SIZE).map_err(|e| {
            BlockStoreError::Serialization(format!("dictionary training failed: {e}"))
        })?;
        self.db
            .put_cf(meta, META_ZSTD_DICT.as_bytes(), dict.as_slice())?;
        Ok(dict)
    }

    /// If dictionary training is enabled, the live dictionary slot is empty, and [`Self::block_count`] is at or above
    /// [`DICT_TRAINING_THRESHOLD`], train once and install into memory.
    pub(crate) fn maybe_train_dictionary(&self) -> Result<(), BlockStoreError> {
        if !self.use_compression_dict {
            return Ok(());
        }
        if self.zstd_dict.read().is_some() {
            return Ok(());
        }
        let meta = self.cf(CF_METADATA)?;
        if self
            .db
            .get_cf(meta, META_ZSTD_DICT.as_bytes())?
            .filter(|b| !b.is_empty())
            .is_some()
        {
            self.init_dictionary()?;
            return Ok(());
        }
        if self.block_count()? < DICT_TRAINING_THRESHOLD {
            return Ok(());
        }
        let dict = self.train_dictionary()?;
        *self.zstd_dict.write() = Some(Arc::new(dict));
        Ok(())
    }
}

pub(crate) fn resolve_zstd_dictionary(
    db: &DB,
    use_compression_dict: bool,
    override_bytes: Option<Vec<u8>>,
) -> Result<Option<Arc<Vec<u8>>>, BlockStoreError> {
    if let Some(bytes) = override_bytes {
        return if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Arc::new(bytes)))
        };
    }
    load_zstd_dict_from_db(db, use_compression_dict)
}

/// Load the trained zstd dictionary from [`CF_METADATA`] / [`META_ZSTD_DICT`].
///
/// Returns `None` when:
/// - `use_compression_dict` is `false` (feature disabled in config).
/// - The [`META_ZSTD_DICT`] key does not exist (no training has occurred).
/// - The stored blob is empty (edge case: metadata key exists but value is zero-length).
///
/// The returned `Arc<Vec<u8>>` is shared between the [`BlockStore`] field `zstd_dict`
/// and all compress/decompress operations, avoiding per-call copies of the ~100 KB dictionary.
///
/// # Called by
///
/// [`resolve_zstd_dictionary`] (at open time) and [`BlockStore::init_dictionary`] (runtime reload).
pub(crate) fn load_zstd_dict_from_db(
    db: &DB,
    use_compression_dict: bool,
) -> Result<Option<Arc<Vec<u8>>>, BlockStoreError> {
    if !use_compression_dict {
        return Ok(None);
    }
    let meta = db
        .cf_handle(CF_METADATA)
        .ok_or_else(|| BlockStoreError::Serialization("missing CF_METADATA".into()))?;
    let Some(blob) = db.get_cf(meta, META_ZSTD_DICT.as_bytes())? else {
        return Ok(None);
    };
    if blob.is_empty() {
        return Ok(None);
    }
    Ok(Some(Arc::new(blob)))
}
