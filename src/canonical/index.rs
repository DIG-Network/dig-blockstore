//! Canonical chain index: height-to-hash resolution, set_canonical, extend_chain.
//!
//! This module owns the canonical chain index operations:
//! - [`BlockStore::get_hash_by_height`]: 3-tier lookup (BTreeMap → mmap → CF_CANONICAL).
//! - [`BlockStore::set_canonical`] / [`BlockStore::set_canonical_batch`]: mark blocks canonical.
//! - [`BlockStore::extend_chain`]: primary block ingestion (put + canonicalize + tip advance).
//!
//! # Requirements
//!
//! - [`CAN-003`](../../docs/requirements/domains/canonical_chain/specs/CAN-003.md) — set_canonical.
//! - [`CAN-004`](../../docs/requirements/domains/canonical_chain/specs/CAN-004.md) — set_canonical_batch.
//! - [`CAN-005`](../../docs/requirements/domains/canonical_chain/specs/CAN-005.md) — extend_chain.
//! - [`CAN-006`](../../docs/requirements/domains/canonical_chain/specs/CAN-006.md) — get_hash_by_height.

use std::sync::atomic::Ordering;

use chia_protocol::Bytes32;
use dig_block::L2Block;
use rocksdb::WriteBatch;

use crate::constants::CF_CANONICAL;
use crate::encoding::{hash_key, height_key};
use crate::error::{BlockStoreError, ERR_MUTATION_READ_ONLY};
use crate::store::BlockStore;
use crate::types::{BlockRecord, ChainTip};

impl BlockStore {
    /// Look up the canonical block hash at a given chain height.
    ///
    /// # Algorithm (dual-layer, [`CAN-006`](../docs/requirements/domains/canonical_chain/specs/CAN-006.md))
    ///
    /// 1. **Hot path — mmap** (`canonical.bin`): O(1) pointer-offset read at `height * 32`.
    ///    ~10ns when the page is OS-cache-resident. Consulted first.
    /// 2. **Cold path — RocksDB** ([`CF_CANONICAL`]): 1-10us key lookup with big-endian
    ///    height key. Used when mmap is unavailable, disabled, or doesn't cover the height.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(hash))` — height has a canonical block.
    /// - `Ok(None)` — height is beyond the chain or was never canonicalized.
    ///
    /// # Chia analogy
    ///
    /// Corresponds to `Blockchain.height_to_hash(height)` in Chia, which reads from
    /// an in-memory `BlockHeightMap` bytearray. DIG adds the durable RocksDB fallback.
    ///
    /// # Derived methods
    ///
    /// [`get_block_by_height`](Self::get_block_by_height),
    /// [`get_header_by_height`](Self::get_header_by_height),
    /// [`get_record_by_height`](Self::get_record_by_height), and
    /// [`get_epoch_block_hashes`](Self::get_epoch_block_hashes) all delegate through this.
    pub fn get_hash_by_height(&self, height: u64) -> Result<Option<Bytes32>, BlockStoreError> {
        // PRN guard: pruned heights should not resolve even if stale mmap data exists
        if height < self.min_retained_height_cached.load(Ordering::Acquire) {
            return Ok(None);
        }
        // Tier 0: CAC-004 in-memory BTreeMap (fastest, O(log n) with no I/O)
        if let Some(hash) = self.canonical_height_cache.read().get(&height).copied() {
            return Ok(Some(hash));
        }
        // Tier 1: mmap canonical.bin (O(1) page-cache read, ~10ns)
        if let Some(arr) = self.canonical_bin.read().read_hash_bytes(height) {
            let hash = Bytes32::new(arr);
            // Populate CAC-004 on read-through
            self.insert_canonical_height_cache(height, hash);
            return Ok(Some(hash));
        }
        // Tier 2: CF_CANONICAL RocksDB (1-10us)
        let cf = self.cf(CF_CANONICAL)?;
        let hk = height_key(height);
        let Some(hash_bytes) = self.db.get_cf(cf, hk.as_slice())? else {
            return Ok(None);
        };
        let arr: [u8; 32] = hash_bytes.as_slice().try_into().map_err(|_| {
            BlockStoreError::Serialization(
                "get_hash_by_height: CF_CANONICAL value must be exactly 32 bytes".into(),
            )
        })?;
        let hash = Bytes32::new(arr);
        // Populate CAC-004 on CF_CANONICAL read-through
        self.insert_canonical_height_cache(height, hash);
        Ok(Some(hash))
    }

    /// Insert a height→hash mapping into the CAC-004 BTreeMap, evicting the lowest
    /// height if the cache exceeds its configured capacity.
    pub(crate) fn insert_canonical_height_cache(&self, height: u64, hash: Bytes32) {
        if self.canonical_height_cache_capacity == 0 {
            return;
        }
        let mut cache = self.canonical_height_cache.write();
        cache.insert(height, hash);
        // Bounded eviction: remove lowest height when over capacity
        while cache.len() > self.canonical_height_cache_capacity {
            if let Some(&lowest) = cache.keys().next() {
                cache.remove(&lowest);
            } else {
                break;
            }
        }
    }

    /// **[`CAN-003`](../docs/requirements/domains/canonical_chain/specs/CAN-003.md)** — Mark an **already stored** block as canonical at its header height.
    ///
    /// **Algorithm (normative order — durable first):**
    /// 1. [`Self::get_record`] to prove the block is known (header row or cache); on miss → [`BlockStoreError::BlockNotInStore`].
    /// 2. [`DB::put_cf`](rocksdb::DB::put_cf) on [`CF_CANONICAL`] with [`height_key`](crate::encoding::height_key)(`height`) → [`hash_key`](crate::encoding::hash_key)(`hash`).
    /// 3. `canonical.bin` update via the same path as [`Self::put_block`] (`canonical_bin` + [`CanonicalDenseFile::write_hash`](crate::canonical::mmap::CanonicalDenseFile::write_hash)); skipped when mmap acceleration is disabled (reopen rebuilds from CF).
    /// 4. Set [`BlockRecord::in_canonical_chain`](crate::types::BlockRecord::in_canonical_chain) = `true` in [`Self::record_cache`] (record remains RAM-only per [`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md)) — **does not** change [`BlockRecord::status`](crate::types::BlockRecord::status); operators may still use [`Self::update_status`](Self::update_status) for lifecycle.
    ///
    /// **Idempotency:** Re-calling with the same hash overwrites CF/mmap with identical bytes and leaves the record flag `true` ([`CAN-003`](../docs/requirements/domains/canonical_chain/specs/CAN-003.md) § Idempotency).
    ///
    /// **Height collisions:** A second call for a **different** hash at the same height overwrites the height index (reorg staging); both blocks must exist in the store.
    ///
    /// **Read-only:** [`BlockStoreError::Serialization`] with [`ERR_MUTATION_READ_ONLY`](crate::error::ERR_MUTATION_READ_ONLY) — same contract as [`Self::put_block`].
    pub fn set_canonical(&self, hash: &Bytes32) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let Some(record) = self.get_record(hash)? else {
            return Err(BlockStoreError::BlockNotInStore(*hash));
        };
        let height = record.height;
        let cf = self.cf(CF_CANONICAL)?;
        self.db
            .put_cf(cf, height_key(height), hash_key(hash).as_slice())?;
        self.canonical_bin.write().extend_write(height, hash)?;
        // CAC-004: populate height→hash cache
        self.insert_canonical_height_cache(height, *hash);
        // CAC-005: populate hash→height cache
        self.hash_to_height_cache.insert(*hash, height);
        if let Some(r) = self.record_cache.lock().get_mut(hash) {
            r.in_canonical_chain = true;
        } else {
            let mut r = record;
            r.in_canonical_chain = true;
            self.record_cache.lock().insert(*hash, r);
        }
        Ok(())
    }

    /// **[`CAN-004`](../docs/requirements/domains/canonical_chain/specs/CAN-004.md)** — Promote **many** already-stored blocks to the canonical height→hash index in one **atomic** RocksDB commit.
    ///
    /// **Why a separate API from [`Self::set_canonical`]:** Reorgs ([`ROR-003`](../docs/requirements/domains/rollback_reorg/specs/ROR-003.md)) must flip many heights at once; a single [`WriteBatch`](rocksdb::WriteBatch) gives all-or-nothing durability in [`CF_CANONICAL`](crate::constants::CF_CANONICAL) ([`NORMATIVE` § CAN-004](../docs/requirements/domains/canonical_chain/NORMATIVE.md#can-004-set_canonical_batch)).
    ///
    /// **Algorithm (matches CAN-004 spec — validate, durable batch, then best-effort hot path):**
    /// 1. **Fail-fast validation:** For each input hash (in order), [`Self::get_record`]. First miss → [`BlockStoreError::BlockNotInStore`] **before** any `WriteBatch` mutation so callers never observe partial CF updates from this method.
    /// 2. **Atomic CF write:** One [`WriteBatch`] with all `height_key(record.height) → hash_key(hash)` rows, then [`DB::write`](rocksdb::DB::write).
    /// 3. **Post-commit:** Same as [`Self::set_canonical`] — [`Self::canonical_bin`]’s mmap writer ([`CanonicalDenseFile::write_hash`](crate::canonical::mmap::CanonicalDenseFile::write_hash) via `extend_write` in `src/canonical/mmap.rs`) per pair, then set [`BlockRecord::in_canonical_chain`](crate::types::BlockRecord::in_canonical_chain) in [`Self::record_cache`] (re-insert on eviction, same as CAN-003).
    ///
    /// **Empty slice:** [`Ok(())] immediately — no I/O ([`CAN-004`](../docs/requirements/domains/canonical_chain/specs/CAN-004.md) acceptance).
    ///
    /// **Crash window:** If the process dies after `db.write` but before mmap/cache finish, [`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md) reopen rebuilds `canonical.bin` from [`CF_CANONICAL`].
    ///
    /// **Read-only:** Same [`BlockStoreError::Serialization`] + [`ERR_MUTATION_READ_ONLY`](crate::error::ERR_MUTATION_READ_ONLY) contract as [`Self::put_block`] / [`Self::set_canonical`].
    pub fn set_canonical_batch(&self, hashes: &[Bytes32]) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        if hashes.is_empty() {
            return Ok(());
        }
        let mut validated: Vec<(Bytes32, BlockRecord)> = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let Some(record) = self.get_record(hash)? else {
                return Err(BlockStoreError::BlockNotInStore(*hash));
            };
            validated.push((*hash, record));
        }
        let cf = self.cf(CF_CANONICAL)?;
        let mut batch = WriteBatch::default();
        for (hash, record) in &validated {
            batch.put_cf(
                &cf,
                height_key(record.height).as_slice(),
                hash_key(hash).as_slice(),
            );
        }
        self.db.write(batch)?;
        for (hash, record) in &validated {
            self.canonical_bin
                .write()
                .extend_write(record.height, hash)?;
            if let Some(r) = self.record_cache.lock().get_mut(hash) {
                r.in_canonical_chain = true;
            } else {
                let mut r = record.clone();
                r.in_canonical_chain = true;
                self.record_cache.lock().insert(*hash, r);
            }
        }
        Ok(())
    }

    /// Primary block ingestion API for normal chain-following operation.
    ///
    /// Combines three operations into one call:
    /// 1. **Store** — [`Self::put`] writes body to `CF_BLOCKS`, header to `CF_HEADERS`,
    ///    and height→hash to `CF_CANONICAL` (canonical=true).
    /// 2. **Tip advance** — [`Self::set_tip`] persists the new chain peak to `CF_METADATA`
    ///    and updates the in-memory `RwLock<Option<ChainTip>>`.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` — block was novel; stored, canonicalized, and tip advanced.
    /// - `Ok(false)` — block hash was already in the store (duplicate); no changes made.
    ///
    /// # Atomicity ([`CAN-005`](../docs/requirements/domains/canonical_chain/specs/CAN-005.md))
    ///
    /// The individual operations are not wrapped in a single RocksDB transaction, but the
    /// ordering ensures safe crash recovery:
    /// - Crash after `put` but before `set_tip`: block is stored and canonical, but tip
    ///   is stale. On restart, tip can be corrected by scanning CF_CANONICAL.
    /// - The duplicate check via [`has_block`](Self::has_block) makes re-ingestion safe.
    ///
    /// # Chia analogy
    ///
    /// Corresponds to the storage portion of `Blockchain.receive_block` →
    /// `BlockStore.add_full_block` in Chia, where the block is stored, the peak is
    /// updated, and the height map is advanced.
    ///
    /// # Errors
    ///
    /// - [`BlockStoreError::Serialization`] with [`ERR_MUTATION_READ_ONLY`] on read-only handles.
    /// - RocksDB or compression errors from the underlying `put` / `set_tip` calls.
    pub fn extend_chain(&self, block: &L2Block) -> Result<bool, BlockStoreError> {
        let hash = block.hash();

        // Duplicate detection: has_block checks cache first, then RocksDB key existence
        // (no deserialization). Matches Chia’s INSERT OR IGNORE semantics.
        if self.has_block(&hash)? {
            return Ok(false);
        }

        // Store block body + header + canonical index entry in one WriteBatch.
        // `put(block, true)` handles CF_BLOCKS, CF_HEADERS, CF_CANONICAL, canonical.bin,
        // and all cache inserts (block_cache, header_cache, record_cache).
        self.put(block, true)?;

        // Advance the chain tip. This is a separate RocksDB write (not in the same
        // WriteBatch as put), but the ordering is safe for crash recovery — see
        // CAN-005 spec § Atomicity Considerations.
        self.set_tip(ChainTip {
            hash,
            height: block.height(),
        })?;

        Ok(true)
    }
}
