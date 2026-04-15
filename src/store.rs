//! `BlockStore` — RocksDB-backed persistent block and chain state.
//!
//! # Architecture
//!
//! This module is the primary entry point for all block persistence in the DIG L2
//! network. It mirrors the storage patterns established by the **Chia blockchain**'s
//! `BlockStore` in `chia-blockchain/chia/consensus/block_store.py`, adapted for Rust
//! and RocksDB instead of Python/SQLite:
//!
//! | Chia Python pattern | DIG Rust equivalent |
//! |---------------------|---------------------|
//! | `full_blocks` SQLite table | [`CF_BLOCKS`](crate::CF_BLOCKS) column family (zstd-compressed bincode) |
//! | `block_records` SQLite table | In-memory [`BlockRecord`](crate::BlockRecord) cache (never persisted; [`TYP-004`]) |
//! | `block_cache: LRUCache[bytes32, FullBlock]` | [`ShardedBlockCache`](crate::cache::sharded::ShardedBlockCache) (sharded LRU, [`CAC-001`]) |
//! | `current_peak` single-row | [`META_TIP`](crate::META_TIP) in [`CF_METADATA`](crate::CF_METADATA) (40-byte [`ChainTip`]) |
//! | `INSERT OR IGNORE` idempotency | [`put_block`](BlockStore::put_block) existence check → `Ok(false)` |
//! | `BlockHeightMap` bytearray | [`CF_CANONICAL`](crate::CF_CANONICAL) + `canonical.bin` mmap ([`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md), [`crate::canonical::mmap`](crate::canonical::mmap)) |
//!
//! # Column family ownership
//!
//! - [`CF_BLOCKS`]: Compressed full block bodies keyed by header hash ([`SER-001`]).
//! - [`CF_HEADERS`]: Uncompressed bincode headers keyed by header hash ([`SER-002`]).
//! - [`CF_CANONICAL`]: Dense height→hash index for the canonical chain ([`CAN-001`]).
//! - [`CF_METADATA`]: Tip, genesis hash, schema version, zstd dictionary ([`TYP-002`]).
//! - [`CF_ATTESTED`]: [`AttestedBlock`](dig_block::AttestedBlock) rows via [`BlockStore::put_attestation`] ([`BLK-009`](../docs/requirements/domains/block_storage/specs/BLK-009.md)).
//! - [`CF_CHECKPOINTS`]: Checkpoint storage ([`CKP-*`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 9).
//!
//! # Three-tier read path
//!
//! Every `get_*` method follows a consistent tiered lookup:
//!
//! 1. **In-memory cache** — sharded LRU for blocks/headers, `HashMap` for records.
//!    Cache hits return clones with zero RocksDB I/O.
//! 2. **RocksDB column family** — on miss, raw bytes are fetched, deserialized, and
//!    inserted back into the cache (read-through).
//! 3. **Absent** — `Ok(None)` when the key does not exist at any tier.
//!
//! # Concurrency model
//!
//! `BlockStore` uses `&self` for all public methods (no `&mut self`). Interior
//! mutability is provided by:
//! - [`parking_lot::RwLock`] for tip and zstd dictionary (read-heavy, rare writes).
//! - [`parking_lot::Mutex`] for the record cache (short critical sections).
//! - [`std::sync::atomic::AtomicUsize`] for instrumentation counters (lock-free).
//! - [`Arc<DB>`] for the RocksDB handle (thread-safe by design).
//!
//! # Requirements trace
//!
//! - [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md) — constructors and lifecycle.
//! - [`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md) — `put_block` / `put`.
//! - [`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) — `get_block` with block cache.
//! - [`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md) — `get_header` with header cache.
//! - [`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md) — `get_record` with layered caching.
//! - [`BLK-005`](../docs/requirements/domains/block_storage/specs/BLK-005.md) — `get_blocks_by_hash` batch retrieval.
//! - [`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) — `stream_blocks_in_range` sequential readahead.
//! - [`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md) — async read wrappers (`get_block_async`, …).
//! - [`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) — async write pipeline (`put_pipelined`, batched `WriteBatch`).
//! - [`BLK-009`](../docs/requirements/domains/block_storage/specs/BLK-009.md) — `put_attestation` / `get_attestation` on [`CF_ATTESTED`].
//! - [`BLK-010`](../docs/requirements/domains/block_storage/specs/BLK-010.md) — `update_status` on in-memory [`BlockRecord`] only.
//! - [`BLK-011`](../docs/requirements/domains/block_storage/specs/BLK-011.md) — `has_block` lightweight existence by hash.
//! - [`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md) — `stats` aggregate [`StorageStats`](crate::types::StorageStats) snapshot.
//! - [`BLK-013`](../docs/requirements/domains/block_storage/specs/BLK-013.md) — `flush` / `compact` maintenance on the shared [`rocksdb::DB`].
//! - [`BLK-014`](../docs/requirements/domains/block_storage/specs/BLK-014.md) — `get_blocks_in_range` and sync `get_block_by_height` over [`CF_CANONICAL`].
//! - [`BLK-015`](../docs/requirements/domains/block_storage/specs/BLK-015.md) — `get_records_in_range` / `get_record_by_height` (header-derived, no block bodies).
//! - [`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md) — dual-layer canonical index (`canonical.bin` + [`CF_CANONICAL`]).
//! - [`CAN-003`](../docs/requirements/domains/canonical_chain/specs/CAN-003.md) — [`set_canonical`](BlockStore::set_canonical) for existing stored blocks.
//! - [`CAN-004`](../docs/requirements/domains/canonical_chain/specs/CAN-004.md) — [`set_canonical_batch`](BlockStore::set_canonical_batch) (single [`WriteBatch`](rocksdb::WriteBatch) for reorg-scale promotion).
//! - [`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) — bincode + zstd block serialization.
//! - [`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md) — bincode-only header serialization.
//! - [`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) — dictionary training and persistence.
//! - [`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md) — sharded block LRU.
//! - [`CAC-002`](../docs/requirements/domains/caching/specs/CAC-002_sharded_header_cache.md) — sharded header LRU.
//! - [`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md) — startup warming.
//!
//! **Spec:** `docs/resources/SPEC.md` §15.1 (constructors), §16 (crate boundary).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use chia_protocol::Bytes32;
use dig_block::{AttestedBlock, BlockStatus, L2Block, L2BlockHeader};
use parking_lot::{Mutex, RwLock};
use rand::seq::SliceRandom;
use rocksdb::{ColumnFamily, Direction, IteratorMode, Options, ReadOptions, WriteBatch, DB};

use crate::cache::sharded::{ShardedBlockCache, ShardedHeaderCache};
use crate::canonical::mmap::CanonicalBin;
use crate::cf_options;
use crate::constants::{
    CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS, CF_HEADERS, CF_METADATA,
    DICT_TARGET_SIZE, DICT_TRAINING_THRESHOLD, META_GENESIS_HASH, META_MIN_HEIGHT, META_TIP,
    META_ZSTD_DICT,
};
use crate::encoding::{decode_height_key, hash_key, height_key};
use crate::error::{
    BlockStoreError, ERR_ASYNC_JOIN_PREFIX, ERR_INIT_GENESIS_ALREADY_INITIALIZED,
    ERR_INIT_GENESIS_READ_ONLY, ERR_MUTATION_READ_ONLY, ERR_OPEN_READONLY_PATH_MISSING_PREFIX,
    ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX,
};
use crate::types::{BlockRecord, ChainTip, StorageStats};
use crate::BlockStoreConfig;

/// One ingress job for [`run_write_pipeline`]: own the [`L2Block`], canonical flag, and per-block ack channel
/// ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) + [`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 5).
///
/// **Ack semantics:** `Ok(true)` means a **new** row was written to [`CF_BLOCKS`]; `Ok(false)` matches [`put_block`](BlockStore::put_block)
/// idempotency (duplicate hash on disk or duplicate within the same batch).
type PipelineJob = (
    L2Block,
    bool,
    oneshot::Sender<Result<bool, BlockStoreError>>,
);

/// Shared RocksDB + cache state behind [`BlockStore`].
///
/// **Why a separate type ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md)):** [`BlockStore`]
/// wraps this struct in [`Arc`] so [`BlockStore::clone`] is a single refcount increment. Async helpers move a clone
/// into [`tokio::task::spawn_blocking`] while preserving one logical store (atomics + caches stay shared).
///
/// Public API remains on [`BlockStore`] via [`std::ops::Deref`]. The type is `pub` so [`Deref::Target`] is
/// well-formed; external crates should still depend on [`BlockStore`] methods only ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) will deepen `Arc` sharing).
#[doc(hidden)]
pub struct BlockStoreInner {
    /// RocksDB handle shared across all operations. Thread-safe via RocksDB's internal locking.
    /// All six column families ([`TYP-001`]) are created at open time.
    db: Arc<DB>,
    /// When `true`, all mutation APIs (`put_block`, `init_genesis`, [`BlockStore::put_attestation`](BlockStore::put_attestation)) return
    /// [`BlockStoreError::Serialization`] with [`ERR_MUTATION_READ_ONLY`].
    /// Set by [`BlockStore::open_readonly`]; cannot be toggled after construction.
    read_only: bool,
    /// In-memory copy of the chain tip from [`META_TIP`] in [`CF_METADATA`].
    /// Updated atomically after [`BlockStore::init_genesis`] and future tip-advance APIs.
    /// Reads use [`RwLock::read`] (very cheap with parking_lot); writes are rare (new blocks).
    tip: RwLock<Option<ChainTip>>,
    /// Count of blocks verified present during cache warming at last [`BlockStore::open`].
    /// Exposed via [`BlockStore::warm_blocks_loaded_count`] for startup diagnostics.
    warm_blocks_loaded: AtomicUsize,
    /// Zstd level for [`BlockStore::serialize_block`] / plain fallback ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) §6).
    compression_level: i32,
    /// When true and [`Self::zstd_dict`] is [`Some`], compress with dictionary ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) precursor).
    use_compression_dict: bool,
    /// Cap passed to [`zstd::bulk::Decompressor::decompress`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) implementation notes).
    max_decompressed_block_bytes: usize,
    /// Trained dictionary loaded from [`META_ZSTD_DICT`] or [`BlockStoreConfig::zstd_dictionary_override`].
    ///
    /// **[`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md):** [`RwLock`] lets
    /// [`BlockStore::maybe_train_dictionary`] publish the first trained dictionary **after** the write that crosses
    /// [`DICT_TRAINING_THRESHOLD`](crate::constants::DICT_TRAINING_THRESHOLD) while keeping [`BlockStore`] on an
    /// immutable `&self` API surface (matches `put`-style ergonomics slated for [`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md)).
    zstd_dict: RwLock<Option<Arc<Vec<u8>>>>,
    /// [`BlockRecord`] rows derived on write; **never** persisted ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md), [`CAC-003`](../docs/requirements/domains/caching/specs/CAC-003.md) precursor).
    ///
    /// **Concurrency:** [`parking_lot::Mutex`] keeps inserts from [`BlockStore::put_block`] / [`BlockStore::init_genesis`] and
    /// lookups from [`BlockStore::get_record`] safe without `&mut self`.
    record_cache: Mutex<HashMap<Bytes32, BlockRecord>>,
    /// Sharded LRU of deserialized [`L2Block`] values ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)).
    block_cache: Arc<ShardedBlockCache>,
    /// Count of RocksDB `get_cf` calls against [`CF_BLOCKS`] issued from [`BlockStore::get_block`] **after** a cache miss.
    ///
    /// **Rationale:** Proves AC §2 “no I/O on hit” in `tests/blk_002_tests.rs`; cheap atomic hot path on miss only.
    /// **Not incremented** by [`BlockStore::get_blocks_by_hash`] (that path uses [`DB::multi_get_cf`](rocksdb::DB::multi_get_cf); see [`BlockStore::cf_blocks_multi_get_batch_count`]).
    cf_blocks_physical_gets: AtomicUsize,
    /// Count of [`rocksdb::DB::multi_get_cf`] **batch invocations** from [`BlockStore::get_blocks_by_hash`] when the input
    /// contains at least one block-cache miss ([`BLK-005`](../docs/requirements/domains/block_storage/specs/BLK-005.md) AC §3).
    ///
    /// **Semantics:** Increments by **at most one per `get_blocks_by_hash` call** that performs RocksDB I/O (all misses
    /// share one `multi_get_cf` round-trip). Stays at zero when every hash hits [`BlockStore::block_cache`] or when `hashes` is empty.
    cf_blocks_multi_get_batches: AtomicUsize,
    /// Count of [`DB::get_cf_opt`](rocksdb::DB::get_cf_opt) calls against [`CF_BLOCKS`] from [`StreamBlocksInRange`]
    /// ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)) after a block-cache miss.
    ///
    /// **Rationale:** Distinct from [`BlockStore::cf_blocks_physical_get_count`] ([`get_block`](BlockStore::get_block)) so tests can
    /// prove cache hits in a streamed range skip redundant block-blob reads ([`tests/blk_006_tests.rs`]).
    cf_blocks_stream_physical_gets: AtomicUsize,
    /// Copy of [`BlockStoreConfig::readahead_size`](crate::BlockStoreConfig::readahead_size) at open time ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §4).
    readahead_size: usize,
    /// Sharded LRU of [`L2BlockHeader`] values ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md)).
    ///
    /// **Separate** from [`Self::block_cache`] per BLK-003 implementation notes (tunables: [`BlockStoreConfig::header_cache_capacity`](crate::BlockStoreConfig::header_cache_capacity)).
    header_cache: Arc<ShardedHeaderCache>,
    /// Count of RocksDB `get_cf` calls against [`CF_HEADERS`] after **both** [`Self::header_cache`] and
    /// [`Self::record_cache`] miss — incremented by [`BlockStore::get_header`] and by [`BlockStore::get_record`] ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md), [`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md)).
    cf_headers_physical_gets: AtomicUsize,
    /// Max jobs per RocksDB [`WriteBatch`] flush ([`BlockStoreConfig::write_pipeline_batch_size`](crate::BlockStoreConfig::write_pipeline_batch_size)).
    pipeline_batch_size: usize,
    /// Partial-batch flush timer ([`BlockStoreConfig::write_pipeline_flush_ms`](crate::BlockStoreConfig::write_pipeline_flush_ms)).
    pipeline_flush_ms: u64,
    /// Bounded channel depth ([`BlockStoreConfig::write_pipeline_channel_capacity`](crate::BlockStoreConfig::write_pipeline_channel_capacity)).
    pipeline_channel_capacity: usize,
    /// Count of successful [`DB::write`](rocksdb::DB::write) calls issued **only** by the pipeline worker ([`tests/blk_008_tests.rs`]).
    pipeline_write_batches: AtomicUsize,
    /// Dense height→hash mmap sidecar (`canonical.bin`) kept in lockstep with [`CF_CANONICAL`] ([`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md)).
    ///
    /// **Reads:** [`parking_lot::RwLock::read`] for [`Self::get_hash_by_height`] (hot path). **Writes:**
    /// [`RwLock::write`] after every successful RocksDB batch that touches the canonical index (`init_genesis`, [`BlockStore::put_block`], pipeline flush).
    canonical_bin: RwLock<CanonicalBin>,
}

/// Primary handle for all block persistence APIs.
///
/// # Chia blockchain analogy
///
/// This struct corresponds to `BlockStore` in `chia-blockchain/chia/consensus/block_store.py`.
/// Where Chia uses a single SQLite `full_blocks` table with Python LRU caches, DIG uses
/// RocksDB column families with Rust sharded LRU caches for higher throughput under
/// concurrent access. The API surface mirrors Chia's: `add_full_block` → [`put_block`](Self::put_block),
/// `get_full_block` → [`get_block`](Self::get_block), `get_block_record` → [`get_record`](Self::get_record).
///
/// # Ownership
///
/// Thin [`Arc`] around [`BlockStoreInner`]: cheap [`Clone`] for [`tokio::task::spawn_blocking`] dispatch ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md)).
/// Field access on `&BlockStore` transparently reaches [`BlockStoreInner`] via [`std::ops::Deref`].
///
/// **Write pipeline ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)):** [`Self::pipeline_tx`]
/// holds the lazy [`mpsc::Sender`] **outside** [`BlockStoreInner`]. The worker task also keeps an [`Arc`] to `inner`
/// for RocksDB; if the sender lived on `inner`, dropping all [`BlockStore`] handles would still leave the sender
/// alive (circular retention), the channel would never close, and AC §8 “flush on shutdown” would not run.
///
/// # Construction
///
/// Use [`BlockStore::open`] for read-write access or [`BlockStore::open_readonly`] for read-only
/// access to an existing database. After construction, call [`BlockStore::init_genesis`] once
/// to initialize a new chain.
pub struct BlockStore {
    inner: Arc<BlockStoreInner>,
    /// Lazy bounded ingress for [`Self::put_pipelined`] — **not** stored on [`BlockStoreInner`] (see struct docs).
    pipeline_tx: Arc<tokio::sync::Mutex<Option<mpsc::Sender<PipelineJob>>>>,
}

impl Clone for BlockStore {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            pipeline_tx: self.pipeline_tx.clone(),
        }
    }
}

impl std::ops::Deref for BlockStore {
    type Target = BlockStoreInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BlockStore {
    /// Open or create a store at `config.path` with all column families ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md), [`TYP-008`](../docs/requirements/domains/storage_types/specs/TYP-008.md)).
    pub fn open(config: BlockStoreConfig) -> Result<Self, BlockStoreError> {
        let compression_level = config.compression_level;
        let use_compression_dict = config.use_compression_dict;
        let max_decompressed_block_bytes = config.max_decompressed_block_bytes;
        let zstd_dictionary_override = config.zstd_dictionary_override.clone();
        // ERR-001 has no `Io` variant; surface directory creation failures as [`BlockStoreError::Serialization`]
        // until the taxonomy adds filesystem errors ([`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md)).
        std::fs::create_dir_all(&config.path).map_err(|e| {
            BlockStoreError::Serialization(format!(
                "filesystem error creating database directory {}: {e}",
                config.path.display()
            ))
        })?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let cfs = cf_options::column_family_descriptors(&config);
        let db = DB::open_cf_descriptors(&opts, &config.path, cfs)?;
        let db = Arc::new(db);
        let canonical_bin =
            RwLock::new(CanonicalBin::open_synced(&db, config.path.as_path(), true)?);
        let zstd_dict =
            resolve_zstd_dictionary(&db, use_compression_dict, zstd_dictionary_override)?;
        let tip = load_tip(&db)?;
        let warm = if config.warm_cache_on_open {
            warm_recent_blocks(&db, &tip, config.warm_cache_depth)?
        } else {
            0
        };
        let readahead_size = config.readahead_size;
        let shards = config.cache_shards.max(1);
        let block_cache = Arc::new(ShardedBlockCache::new(config.block_cache_capacity, shards));
        let header_cache = Arc::new(ShardedHeaderCache::new(
            config.header_cache_capacity,
            shards,
        ));
        Ok(Self {
            inner: Arc::new(BlockStoreInner {
                db,
                read_only: false,
                tip: RwLock::new(tip),
                warm_blocks_loaded: AtomicUsize::new(warm),
                compression_level,
                use_compression_dict,
                max_decompressed_block_bytes,
                zstd_dict: RwLock::new(zstd_dict),
                record_cache: Mutex::new(HashMap::new()),
                block_cache,
                cf_blocks_physical_gets: AtomicUsize::new(0),
                cf_blocks_multi_get_batches: AtomicUsize::new(0),
                cf_blocks_stream_physical_gets: AtomicUsize::new(0),
                readahead_size,
                header_cache,
                cf_headers_physical_gets: AtomicUsize::new(0),
                pipeline_batch_size: config.write_pipeline_batch_size.max(1),
                pipeline_flush_ms: config.write_pipeline_flush_ms.max(1),
                pipeline_channel_capacity: config.write_pipeline_channel_capacity.max(1),
                pipeline_write_batches: AtomicUsize::new(0),
                canonical_bin,
            }),
            pipeline_tx: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// Open an existing database read-only; fails if `path` does not exist ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub fn open_readonly(path: impl AsRef<Path>) -> Result<Self, BlockStoreError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(BlockStoreError::Serialization(format!(
                "{ERR_OPEN_READONLY_PATH_MISSING_PREFIX}{}",
                path.display()
            )));
        }
        let opts = Options::default();
        // CF option structs must match how the DB was created; tests use STR-005 `test_config`, which
        // mirrors [`BlockStoreConfig::default`] for `enable_blob_db` ([`TYP-003`](../../docs/requirements/domains/storage_types/specs/TYP-003.md)).
        let readonly_cfg = BlockStoreConfig {
            path: path.to_path_buf(),
            ..BlockStoreConfig::default()
        };
        let compression_level = readonly_cfg.compression_level;
        let use_compression_dict = readonly_cfg.use_compression_dict;
        let max_decompressed_block_bytes = readonly_cfg.max_decompressed_block_bytes;
        let zstd_dictionary_override = readonly_cfg.zstd_dictionary_override.clone();
        let cfs = cf_options::column_family_descriptors(&readonly_cfg);
        let db = DB::open_cf_descriptors_read_only(&opts, path, cfs, false)?;
        let db = Arc::new(db);
        let canonical_bin = RwLock::new(CanonicalBin::open_synced(&db, path, false)?);
        let zstd_dict =
            resolve_zstd_dictionary(&db, use_compression_dict, zstd_dictionary_override)?;
        let tip = load_tip(&db)?;
        let readahead_size = readonly_cfg.readahead_size;
        let shards = readonly_cfg.cache_shards.max(1);
        let block_cache = Arc::new(ShardedBlockCache::new(
            readonly_cfg.block_cache_capacity,
            shards,
        ));
        let header_cache = Arc::new(ShardedHeaderCache::new(
            readonly_cfg.header_cache_capacity,
            shards,
        ));
        Ok(Self {
            inner: Arc::new(BlockStoreInner {
                db,
                read_only: true,
                tip: RwLock::new(tip),
                warm_blocks_loaded: AtomicUsize::new(0),
                compression_level,
                use_compression_dict,
                max_decompressed_block_bytes,
                zstd_dict: RwLock::new(zstd_dict),
                record_cache: Mutex::new(HashMap::new()),
                block_cache,
                cf_blocks_physical_gets: AtomicUsize::new(0),
                cf_blocks_multi_get_batches: AtomicUsize::new(0),
                cf_blocks_stream_physical_gets: AtomicUsize::new(0),
                readahead_size,
                header_cache,
                cf_headers_physical_gets: AtomicUsize::new(0),
                pipeline_batch_size: readonly_cfg.write_pipeline_batch_size.max(1),
                pipeline_flush_ms: readonly_cfg.write_pipeline_flush_ms.max(1),
                pipeline_channel_capacity: readonly_cfg.write_pipeline_channel_capacity.max(1),
                pipeline_write_batches: AtomicUsize::new(0),
                canonical_bin,
            }),
            pipeline_tx: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// Initialize genesis: empty store only; atomic [`WriteBatch`] ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub fn init_genesis(&self, block: &L2Block) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_INIT_GENESIS_READ_ONLY.into(),
            ));
        }
        let meta = self.cf(CF_METADATA)?;
        if self.db.get_cf(meta, META_TIP.as_bytes())?.is_some()
            || self
                .db
                .get_cf(meta, META_GENESIS_HASH.as_bytes())?
                .is_some()
        {
            return Err(BlockStoreError::Serialization(
                ERR_INIT_GENESIS_ALREADY_INITIALIZED.into(),
            ));
        }
        let hash = block.hash();
        if block.height() != 0 {
            return Err(BlockStoreError::Serialization(format!(
                "init_genesis: genesis block height must be 0, got {}",
                block.height()
            )));
        }
        // [`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md): bincode + zstd (dictionary when configured).
        let compressed = self.serialize_block(block)?;
        // [`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md): headers are bincode-only in `CF_HEADERS`.
        let header_bytes = Self::serialize_header(&block.header)?;
        let tip = ChainTip { hash, height: 0 };
        let mut batch = WriteBatch::default();
        let cf_b = self.cf(CF_BLOCKS)?;
        let cf_h = self.cf(CF_HEADERS)?;
        let cf_c = self.cf(CF_CANONICAL)?;
        // [`hash_key`] returns `[u8; 32]`; use `.as_slice()` (not `.as_ref()`) so RocksDB keys resolve to
        // `&[u8]` without ambiguous `AsRef` when the `bitcoin` crate is also in the dependency graph.
        batch.put_cf(cf_b, hash_key(&hash).as_slice(), &compressed);
        batch.put_cf(cf_h, hash_key(&hash).as_slice(), &header_bytes);
        batch.put_cf(cf_c, height_key(0), hash_key(&hash).as_slice());
        batch.put_cf(meta, META_TIP.as_bytes(), tip.to_bytes().as_slice());
        batch.put_cf(meta, META_GENESIS_HASH.as_bytes(), hash.as_ref());
        self.db.write(batch)?;
        self.canonical_bin.write().extend_write(0, &hash)?;
        *self.tip.write() = Some(tip);
        let record = BlockRecord::from_header(&block.header, BlockStatus::Validated);
        self.record_cache.lock().insert(hash, record);
        self.block_cache.insert(hash, block.clone());
        self.header_cache.insert(hash, block.header.clone());
        self.maybe_train_dictionary()?;
        Ok(())
    }

    /// **Diagnostics / tests:** Disable the mmap acceleration layer so height→hash resolution uses [`CF_CANONICAL`]
    /// only until the next [`BlockStore::open`] ([`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md) test plan: mmap fallback).
    ///
    /// **Production:** Do not call — the next process restart re-syncs `canonical.bin` from RocksDB anyway.
    pub fn disable_canonical_bin_acceleration(&self) {
        self.canonical_bin.write().disable();
    }

    /// Current chain tip — hash and height of the highest canonical block.
    ///
    /// Returns the in-memory cached copy loaded from [`META_TIP`](crate::META_TIP) at startup
    /// and updated by [`BlockStore::set_tip`], [`BlockStore::init_genesis`], and future
    /// `extend_chain` / `rollback_to_height` APIs.
    ///
    /// # Performance
    ///
    /// This is a hot-path accessor queried on every block ingestion for parent-hash validation.
    /// The [`parking_lot::RwLock::read`] is lock-free on the uncontended fast path (~2-5ns).
    /// No RocksDB I/O occurs.
    ///
    /// # Chia analogy
    ///
    /// Corresponds to `BlockStore.get_peak()` in Chia's `block_store.py`.
    ///
    /// **Requirement:** [`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md).
    pub fn tip(&self) -> Option<ChainTip> {
        *self.tip.read()
    }

    /// Convenience accessor for the current canonical chain height.
    ///
    /// Returns `tip().map(|t| t.height)` — `None` when the store has no tip (before genesis),
    /// `Some(0)` after genesis, `Some(n)` after extending to height `n`.
    ///
    /// **Requirement:** [`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md) § Accessors.
    #[must_use]
    pub fn height(&self) -> Option<u64> {
        self.tip().map(|t| t.height)
    }

    /// Persist a new chain tip to [`CF_METADATA`](crate::CF_METADATA) / [`META_TIP`](crate::META_TIP)
    /// and update the in-memory cache.
    ///
    /// # Encoding
    ///
    /// The value written is exactly 40 bytes: `hash (32 bytes, raw Bytes32) || height (8 bytes, little-endian u64)`.
    /// This matches [`ChainTip::to_bytes()`](crate::ChainTip::to_bytes) and the
    /// [`TYP-006`](../docs/requirements/domains/storage_types/specs/TYP-006.md) wire format.
    ///
    /// # Ordering
    ///
    /// RocksDB write is performed **before** updating the in-memory `RwLock`. If the write
    /// fails, the in-memory tip remains unchanged (no stale state visible to concurrent readers).
    ///
    /// # Errors
    ///
    /// - [`BlockStoreError::Serialization`] with [`ERR_MUTATION_READ_ONLY`](crate::ERR_MUTATION_READ_ONLY)
    ///   when called on a read-only handle.
    /// - [`BlockStoreError::RocksDb`] on write failure.
    ///
    /// # Update points ([`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md))
    ///
    /// | Operation | New Tip |
    /// |-----------|---------|
    /// | `extend_chain` (CAN-005) | Newly added block |
    /// | `rollback_to_height` (ROR-001) | Block at target height |
    /// | `apply_reorg` (ROR-003) | Last block in new chain |
    /// | `init_genesis` (STR-004) | Genesis block (height 0) |
    pub fn set_tip(&self, tip: ChainTip) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let cf = self.cf(CF_METADATA)?;
        // Write the 40-byte encoding to CF_METADATA before updating in-memory state.
        // On failure, the in-memory tip remains at the old value (no stale state).
        self.db
            .put_cf(cf, META_TIP.as_bytes(), tip.to_bytes().as_slice())?;
        *self.tip.write() = Some(tip);
        Ok(())
    }

    /// Blocks successfully verified present while warming on last [`Self::open`] ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md) / [`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
    pub fn warm_blocks_loaded_count(&self) -> usize {
        self.warm_blocks_loaded.load(Ordering::Relaxed)
    }

    /// Serialize a block header for [`CF_HEADERS`] ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
    ///
    /// **Write path (normative):** `L2BlockHeader` → [`bincode::serialize`] → raw bytes (no zstd). Headers are
    /// small and read on every chain walk; skipping compression avoids framing overhead and decode latency
    /// on the hot path ([`NORMATIVE.md` § SER-002](../docs/requirements/domains/serialization/NORMATIVE.md)).
    ///
    /// **Errors:** [`BlockStoreError::Serialization`] — same variant as corrupt block payloads so upper
    /// layers can treat “bytes unusable” uniformly until ERR-* adds finer codes.
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
    fn decompress_block_payload(&self, compressed: &[u8]) -> std::io::Result<Vec<u8>> {
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
    ///
    /// **Write path:** [`Self::put_block`] / [`Self::init_genesis`] insert fresh values so steady-state reads hit RAM.
    pub fn get_block(&self, hash: &Bytes32) -> Result<Option<L2Block>, BlockStoreError> {
        if let Some(block) = self.block_cache.get_clone(hash) {
            return Ok(Some(block));
        }
        let cf = self.cf(CF_BLOCKS)?;
        self.cf_blocks_physical_gets.fetch_add(1, Ordering::Relaxed);
        let raw_opt = self.db.get_cf(cf, hash_key(hash).as_slice())?;
        let Some(raw) = raw_opt else {
            return Ok(None);
        };
        let block = self.deserialize_block(&raw)?;
        self.block_cache.insert(*hash, block.clone());
        self.header_cache.insert(*hash, block.header.clone());
        Ok(Some(block))
    }

    /// **[`BLK-011`](../docs/requirements/domains/block_storage/specs/BLK-011.md)** — Whether any persisted row exists for `hash` **without** decoding zstd or bincode ([`NORMATIVE.md` § BLK-011](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-011-has-block-has_block)).
    ///
    /// **Cache first (AC §2):** [`Self::block_cache`] and [`Self::header_cache`] are consulted via [`ShardedLruCache::contains`](crate::cache::sharded::ShardedLruCache::contains) ([`LruCache::peek`](lru::LruCache::peek) — no LRU promotion).
    ///
    /// **RocksDB (AC §1):** If both caches miss, probe [`CF_HEADERS`] then [`CF_BLOCKS`] using [`hash_key`](crate::encoding::hash_key) (same key layout as [`Self::put_block`]). The second probe covers edge cases where only a body row exists; normal [`Self::put_block`] writes both families together ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md)).
    ///
    /// **No deserialize / decompress (AC §3):** Uses only [`DB::get_cf`](rocksdb::DB::get_cf) presence checks — returned bytes are discarded without calling [`Self::deserialize_block`] or [`Self::deserialize_header`].
    ///
    /// **Instrumentation:** This path does **not** increment [`Self::cf_blocks_physical_get_count`] (that counter remains exclusive to [`Self::get_block`]) so tests can prove cache-fast paths avoid the heavy read API ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) counter semantics).
    pub fn has_block(&self, hash: &Bytes32) -> Result<bool, BlockStoreError> {
        if self.block_cache.contains(hash) || self.header_cache.contains(hash) {
            return Ok(true);
        }
        let key = hash_key(hash);
        let cf_h = self.cf(CF_HEADERS)?;
        if self.db.get_cf(cf_h, key.as_slice())?.is_some() {
            return Ok(true);
        }
        let cf_b = self.cf(CF_BLOCKS)?;
        Ok(self.db.get_cf(cf_b, key.as_slice())?.is_some())
    }

    /// **[`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md)** — Aggregate [`StorageStats`](crate::types::StorageStats) for monitoring / diagnostics ([`TYP-007`](../docs/requirements/domains/storage_types/specs/TYP-007.md), [`NORMATIVE` BLK-012](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-012-storage-statistics-stats)).
    ///
    /// **Row counts:** Each `*_count` field is the number of keys in the corresponding column family from a linear
    /// [`DB::iterator_cf`](rocksdb::DB::iterator_cf) scan ([`CF_BLOCKS`], [`CF_HEADERS`], [`CF_CANONICAL`],
    /// [`CF_CHECKPOINTS`], [`CF_ATTESTED`]). This is **exact** for current store sizes (typical node counts) and
    /// matches NORMATIVE wording (“reflects the total number of entries”). If full scans become too costly at scale,
    /// a future revision may offer `rocksdb.estimate-num-keys` behind configuration with documented error bounds.
    ///
    /// **Tip / pruning:** [`StorageStats::tip_height`] mirrors [`Self::tip`] (RAM snapshot loaded from [`META_TIP`] on open, updated by [`Self::init_genesis`] today). [`Self::put_block`] does not yet advance the tip ([`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md)); operators should not assume `tip_height == max(block heights)` until chain-tip APIs land. [`StorageStats::min_height`] reads
    /// [`META_MIN_HEIGHT`] in [`CF_METADATA`] as **8 bytes little-endian** `u64` ([`storage_types/NORMATIVE`](../docs/requirements/domains/storage_types/NORMATIVE.md)); missing key means no prune watermark yet ([`PRN-004`](../docs/requirements/domains/pruning/specs/PRN-004_min_retained_height_tracking.md)).
    ///
    /// **Disk estimate:** [`StorageStats::total_size_bytes`] sums per-CF RocksDB property `rocksdb.estimate-live-data-size`
    /// (live SST + memtable footprint estimate). It is **not** a byte-exact `du` of the directory; callers should treat
    /// it as an order-of-magnitude health signal; use [`Self::flush`] / [`Self::compact`] before relying on
    /// filesystem-level durability or space reclamation ([`BLK-013`](../docs/requirements/domains/block_storage/specs/BLK-013.md)).
    pub fn stats(&self) -> Result<StorageStats, BlockStoreError> {
        Ok(StorageStats {
            block_count: self.count_cf_entries(CF_BLOCKS)?,
            canonical_block_count: self.count_cf_entries(CF_CANONICAL)?,
            header_count: self.count_cf_entries(CF_HEADERS)?,
            checkpoint_count: self.count_cf_entries(CF_CHECKPOINTS)?,
            attested_count: self.count_cf_entries(CF_ATTESTED)?,
            tip_height: self.tip().map(|t| t.height),
            min_height: self.read_min_retained_height()?,
            total_size_bytes: self.sum_cf_live_data_size_estimates()?,
        })
    }

    /// Exact key count for `cf_name` — used by [`Self::stats`] ([`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md)).
    fn count_cf_entries(&self, cf_name: &'static str) -> Result<u64, BlockStoreError> {
        let cf = self.cf(cf_name)?;
        let mut n = 0u64;
        for entry in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_k, _v) = entry?;
            n += 1;
        }
        Ok(n)
    }

    /// Sum RocksDB `rocksdb.estimate-live-data-size` across all user column families ([`BLK-012`](../docs/requirements/domains/block_storage/specs/BLK-012.md) § Field Population).
    fn sum_cf_live_data_size_estimates(&self) -> Result<u64, BlockStoreError> {
        const PROP: &str = "rocksdb.estimate-live-data-size";
        let mut sum = 0u64;
        for name in [
            CF_BLOCKS,
            CF_HEADERS,
            CF_CANONICAL,
            CF_METADATA,
            CF_ATTESTED,
            CF_CHECKPOINTS,
        ] {
            let cf = self.cf(name)?;
            if let Some(v) = self.db.property_int_value_cf(cf, PROP)? {
                sum = sum.saturating_add(v);
            }
        }
        Ok(sum)
    }

    /// Read persisted minimum retained height, if pruning has written [`META_MIN_HEIGHT`].
    fn read_min_retained_height(&self) -> Result<Option<u64>, BlockStoreError> {
        let meta = self.cf(CF_METADATA)?;
        let Some(bytes) = self.db.get_cf(meta, META_MIN_HEIGHT.as_bytes())? else {
            return Ok(None);
        };
        let arr: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
            BlockStoreError::Serialization(format!(
                "stats: META_MIN_HEIGHT value must be exactly 8 bytes (little-endian u64), got {} bytes",
                bytes.len()
            ))
        })?;
        Ok(Some(u64::from_le_bytes(arr)))
    }

    /// **[`BLK-013`](../docs/requirements/domains/block_storage/specs/BLK-013.md)** — Persist buffered engine state
    /// ([`NORMATIVE` BLK-013](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-013-flush-and-compact)).
    ///
    /// **Semantics:** First `rocksdb::DB::flush_wal(true)` so the write-ahead log is **synced** through the OS to
    /// stable storage, then [`rocksdb::DB::flush`] to flush **all** column-family
    /// memtables to SST files. Together this matches operators’ “make my recent writes durable” intent while staying
    /// close to the BLK-013 spec snippet (which only showed `flush()` — WAL sync is required by NORMATIVE item 1’s
    /// “WAL flush” wording).
    ///
    /// **Logical state:** Does not mutate dig-blockstore caches, tip, or row keys — only RocksDB I/O.
    ///
    /// **Errors:** Any [`rocksdb::Error`] maps to [`BlockStoreError::RocksDb`] ([`ERR-002`](../docs/requirements/domains/error_types/specs/ERR-002_error_from_conversions.md)).
    pub fn flush(&self) -> Result<(), BlockStoreError> {
        self.db.flush_wal(true)?;
        self.db.flush()?;
        Ok(())
    }

    /// **[`BLK-013`](../docs/requirements/domains/block_storage/specs/BLK-013.md)** — Request manual compaction on
    /// **every** column family in [`crate::constants::ALL_COLUMN_FAMILIES`] ([`TYP-001`](../docs/requirements/domains/storage_types/specs/TYP-001.md)).
    ///
    /// **Implementation:** For each family, [`DB::compact_range_cf`](rocksdb::DB::compact_range_cf) with a `None`
    /// key range compacts the **entire** keyspace (RocksDB schedules background work). The rust-rocksdb binding
    /// returns `()` from `compact_range_cf` (errors surface asynchronously); callers use this for **space reclamation**
    /// and read amplification tuning, not as a transactional barrier.
    ///
    /// **Logical state:** Compaction does not delete live keys written by [`Self::put_block`] / [`Self::init_genesis`];
    /// it merges SSTables. Same error mapping as [`Self::flush`] if future APIs gain fallible compaction entry points.
    pub fn compact(&self) -> Result<(), BlockStoreError> {
        for &name in crate::constants::ALL_COLUMN_FAMILIES {
            let cf = self.cf(name)?;
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
        Ok(())
    }

    /// Batch-fetch blocks by hash ([`BLK-005`](../docs/requirements/domains/block_storage/specs/BLK-005.md)).
    ///
    /// **Algorithm**
    /// 1. For each input hash in order, clone from [`Self::block_cache`] when present ([`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
    /// 2. Collect all cache misses; if non-empty, issue **one** [`rocksdb::DB::multi_get_cf`] over [`CF_BLOCKS`]
    ///    (same `(cf, key)` pattern as [`Self::get_block`], [`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) payloads).
    /// 3. For each returned blob: [`Self::deserialize_block`], then insert into [`Self::block_cache`] and [`Self::header_cache`]
    ///    (mirrors single-key read-through in [`Self::get_block`]).
    ///
    /// **Ordering:** Output `Vec` index `i` always corresponds to `hashes[i]` (per NORMATIVE BLK-005 §5).
    ///
    /// **Missing keys:** `Ok(None)` at that index; RocksDB row absent still consumes one slot in the `multi_get` result vector.
    ///
    /// **Empty input:** Returns `Ok(vec![])` without touching RocksDB.
    ///
    /// **Chunking:** Very large batches stay single-call for now ([`BLK-005.md`](../docs/requirements/domains/block_storage/specs/BLK-005.md) implementation notes); future work may split to bound peak memory.
    pub fn get_blocks_by_hash(
        &self,
        hashes: &[Bytes32],
    ) -> Result<Vec<Option<L2Block>>, BlockStoreError> {
        let mut results: Vec<Option<L2Block>> = vec![None; hashes.len()];
        let mut miss_indices: Vec<usize> = Vec::new();
        for (i, hash) in hashes.iter().enumerate() {
            if let Some(block) = self.block_cache.get_clone(hash) {
                results[i] = Some(block);
            } else {
                miss_indices.push(i);
            }
        }
        if miss_indices.is_empty() {
            return Ok(results);
        }
        let cf = self.cf(CF_BLOCKS)?;
        self.cf_blocks_multi_get_batches
            .fetch_add(1, Ordering::Relaxed);
        let keys: Vec<[u8; 32]> = miss_indices
            .iter()
            .map(|&idx| *hash_key(&hashes[idx]))
            .collect();
        let db_results = self
            .db
            .multi_get_cf(keys.iter().map(|k| (cf, k.as_slice())));
        for (j, db_result) in db_results.into_iter().enumerate() {
            let idx = miss_indices[j];
            let maybe_raw = db_result?;
            let Some(raw) = maybe_raw else {
                continue;
            };
            let block = self.deserialize_block(&raw)?;
            self.block_cache.insert(hashes[idx], block.clone());
            self.header_cache.insert(hashes[idx], block.header.clone());
            results[idx] = Some(block);
        }
        Ok(results)
    }

    /// Drop a single entry from the in-memory block LRU — **no RocksDB writes** ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) test plan: simulate eviction).
    pub fn invalidate_block_cache_entry(&self, hash: &Bytes32) {
        self.block_cache.remove(hash);
    }

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
        if let Some(arr) = self.canonical_bin.read().read_hash_bytes(height) {
            return Ok(Some(Bytes32::new(arr)));
        }
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
        Ok(Some(Bytes32::new(arr)))
    }

    /// Look up the canonical block at `height` ([`CAN-006`](../docs/requirements/domains/canonical_chain/specs/CAN-006.md) precursor, [`BLK-014`](../docs/requirements/domains/block_storage/specs/BLK-014.md) building block).
    ///
    /// **Algorithm:** [`Self::get_hash_by_height`] (mmap then [`CF_CANONICAL`]) → [`Self::get_block`]
    /// ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) decompress + cache).
    ///
    /// **Returns:** `Ok(None)` when the height index is absent **or** when the hash is indexed but the body row is
    /// missing (same as [`Self::get_block`] returning `None`).
    ///
    /// **Threading:** Safe on any thread; performs synchronous RocksDB + zstd work — use [`Self::get_block_by_height_async`]
    /// from async contexts that must not block the runtime ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md)).
    pub fn get_block_by_height(&self, height: u64) -> Result<Option<L2Block>, BlockStoreError> {
        let Some(hash) = self.get_hash_by_height(height)? else {
            return Ok(None);
        };
        self.get_block(&hash)
    }

    /// **[`BLK-014`](../docs/requirements/domains/block_storage/specs/BLK-014.md)** — Collect canonical [`L2Block`]s for
    /// heights in `[start_height, end_height]` inclusive ([`NORMATIVE` BLK-014](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-014-get-blocks-in-range-get_blocks_in_range)).
    ///
    /// **Semantics:** Ascending height order; `start_height > end_height` ⇒ empty `Vec` (not an error); any height with
    /// no canonical row or no retrievable body is **omitted** (gaps and “beyond tip” behave the same — fewer results).
    ///
    /// **vs [`Self::stream_blocks_in_range`] ([`BLK-006`]):** This API eagerly builds a `Vec` with simple point lookups per height.
    /// [`StreamBlocksInRange`] is better for large scans (single readahead iterator over [`CF_CANONICAL`]).
    pub fn get_blocks_in_range(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<L2Block>, BlockStoreError> {
        if start_height > end_height {
            return Ok(Vec::new());
        }
        let mut blocks = Vec::with_capacity((end_height - start_height + 1) as usize);
        for height in start_height..=end_height {
            if let Some(block) = self.get_block_by_height(height)? {
                blocks.push(block);
            }
        }
        Ok(blocks)
    }

    /// Look up the canonical [`BlockRecord`] at `height` ([`BLK-015`](../docs/requirements/domains/block_storage/specs/BLK-015.md), [`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md)).
    ///
    /// **Resolution:** Same [`CF_CANONICAL`] → [`Bytes32`] step as [`Self::get_block_by_height`], then [`Self::get_record`]
    /// so misses load **bincode headers only** from [`CF_HEADERS`] ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)) — no zstd frame read from [`CF_BLOCKS`].
    ///
    /// **Returns:** `Ok(None)` when the height index is missing or when neither [`CF_HEADERS`] nor caches can supply a header.
    ///
    /// **Canonical resolution:** Same [`Self::get_hash_by_height`] dual layer as [`Self::get_block_by_height`] ([`CAN-001`](../docs/requirements/domains/canonical_chain/specs/CAN-001.md)).
    pub fn get_record_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockRecord>, BlockStoreError> {
        let Some(hash) = self.get_hash_by_height(height)? else {
            return Ok(None);
        };
        self.get_record(&hash)
    }

    /// Look up the canonical header at `height` ([`CAN-006`](../docs/requirements/domains/canonical_chain/specs/CAN-006.md)).
    ///
    /// **Algorithm:** [`Self::get_hash_by_height`] → [`Self::get_header`]
    /// ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md) cache + bincode).
    ///
    /// **Returns:** `Ok(None)` when the height is not canonical or the header row is absent.
    ///
    /// **Lighter than `get_block_by_height`:** Headers are uncompressed bincode (~700 bytes)
    /// versus full block bodies (zstd decompression + larger payload). Use this when only
    /// header fields are needed (e.g., parent-hash walks, timestamp checks).
    pub fn get_header_by_height(
        &self,
        height: u64,
    ) -> Result<Option<L2BlockHeader>, BlockStoreError> {
        let Some(hash) = self.get_hash_by_height(height)? else {
            return Ok(None);
        };
        self.get_header(&hash)
    }

    /// Collect canonical block hashes for all heights in the given epoch.
    ///
    /// **Algorithm:** Uses [`dig_epoch::epoch_height_range`] to determine the `[start, end]`
    /// height range, then calls [`Self::get_hash_by_height`] for each height. Stops early
    /// when a height returns `None` (chain hasn't reached that height yet).
    ///
    /// **Returns:** A `Vec<Bytes32>` containing one hash per canonical height in the epoch,
    /// in ascending height order. May be shorter than `BLOCKS_PER_EPOCH` if the chain is
    /// still growing into the epoch, or empty if the epoch is entirely beyond the chain tip.
    ///
    /// **Requirement:** [`CAN-006`](../docs/requirements/domains/canonical_chain/specs/CAN-006.md) § Epoch Block Hashes.
    pub fn get_epoch_block_hashes(&self, epoch: u64) -> Result<Vec<Bytes32>, BlockStoreError> {
        let (start, end) = dig_epoch::epoch_height_range(epoch);
        let mut hashes = Vec::new();
        for height in start..=end {
            if let Some(hash) = self.get_hash_by_height(height)? {
                hashes.push(hash);
            } else {
                break; // chain hasn't reached this height yet
            }
        }
        Ok(hashes)
    }

    /// **[`BLK-015`](../docs/requirements/domains/block_storage/specs/BLK-015.md)** — Materialize canonical [`BlockRecord`]s for `[start_height, end_height]` inclusive ([`NORMATIVE` BLK-015](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-015-get-records-in-range-get_records_in_range)).
    ///
    /// **Semantics:** Matches [`Self::get_blocks_in_range`] ordering and gap rules ([`BLK-014`](../docs/requirements/domains/block_storage/specs/BLK-014.md)), but each row comes from [`Self::get_record_by_height`] so operators avoid zstd decompression on the hot path ([`BLK-015`](../docs/requirements/domains/block_storage/specs/BLK-015.md) § Specification).
    ///
    /// **Cache interaction:** [`Self::get_record`] may insert derived rows into [`Self::record_cache`] / [`Self::header_cache`];
    /// repeated scans therefore become cheaper, mirroring single-hash lookups ([`CAC-003`](../docs/requirements/domains/caching/specs/CAC-003.md) precursor).
    pub fn get_records_in_range(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<BlockRecord>, BlockStoreError> {
        if start_height > end_height {
            return Ok(Vec::new());
        }
        let mut records = Vec::with_capacity((end_height - start_height + 1) as usize);
        for height in start_height..=end_height {
            if let Some(record) = self.get_record_by_height(height)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// How many times [`Self::get_block`] reached RocksDB [`CF_BLOCKS`] after a cache miss (includes `Ok(None)` probes).
    ///
    /// **Tests / ops:** [`tests/blk_002_tests.rs`] asserts hits add zero; misses increment exactly once per call.
    pub fn cf_blocks_physical_get_count(&self) -> u64 {
        self.cf_blocks_physical_gets.load(Ordering::Relaxed) as u64
    }

    /// How many times [`Self::get_blocks_by_hash`] invoked [`rocksdb::DB::multi_get_cf`] because at least one hash missed
    /// [`Self::block_cache`] ([`BLK-005`](../docs/requirements/domains/block_storage/specs/BLK-005.md); see [`tests/blk_005_tests.rs`]).
    #[inline]
    pub fn cf_blocks_multi_get_batch_count(&self) -> u64 {
        self.cf_blocks_multi_get_batches.load(Ordering::Relaxed) as u64
    }

    /// RocksDB readahead hint (bytes) copied from [`BlockStoreConfig::readahead_size`](crate::BlockStoreConfig::readahead_size) at open ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §4).
    #[must_use]
    pub fn readahead_size(&self) -> usize {
        self.readahead_size
    }

    /// How many times [`StreamBlocksInRange`] issued [`DB::get_cf_opt`](rocksdb::DB::get_cf_opt) against [`CF_BLOCKS`]
    /// after a block-cache miss while streaming ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md); [`tests/blk_006_tests.rs`]).
    #[must_use]
    pub fn cf_blocks_stream_physical_get_count(&self) -> u64 {
        self.cf_blocks_stream_physical_gets.load(Ordering::Relaxed) as u64
    }

    /// Build [`ReadOptions`] for sequential [`CF_BLOCKS`] reads inside [`StreamBlocksInRange`] ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §3).
    fn blocks_stream_read_options(&self) -> ReadOptions {
        let mut o = ReadOptions::default();
        o.set_readahead_size(self.readahead_size);
        o
    }

    /// Stream canonical blocks from height `start` through `end` inclusive ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).
    ///
    /// **Phase 1 — canonical walk:** [`DB::iterator_cf_opt`](rocksdb::DB::iterator_cf_opt) over [`CF_CANONICAL`] with
    /// [`ReadOptions::set_readahead_size`](rocksdb::ReadOptions::set_readahead_size) and iterate bounds
    /// ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md) big-endian order).
    ///
    /// **Phase 2 — lazy bodies:** The returned [`StreamBlocksInRange`] walks the captured `(height, hash)` slice and,
    /// for each entry, serves [`ShardedBlockCache`](crate::cache::sharded::ShardedBlockCache) hits without RocksDB, or
    /// [`DB::get_cf_opt`](rocksdb::DB::get_cf_opt) on [`CF_BLOCKS`] with the same readahead hint (separate [`ReadOptions`]
    /// instance so canonical and block reads each carry the configured hint).
    ///
    /// **Why two phases:** A live RocksDB iterator over [`CF_CANONICAL`] cannot coexist with mutable/immutable borrows
    /// of [`Self::block_cache`] / decompressors on every `Iterator::next` without self-referential structs; materializing
    /// the height→hash list preserves **readahead on the canonical scan** while keeping the public API safe and `'static`-free.
    ///
    /// **Errors:** Missing [`CF_BLOCKS`] row for a canonical hash yields [`BlockStoreError::BlockNotFound`] from the stream
    /// ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §6). Malformed canonical keys/values map to [`BlockStoreError::Serialization`].
    ///
    /// **Empty / inverted range:** If `start > end`, returns an iterator that yields immediately without I/O.
    pub fn stream_blocks_in_range(
        &self,
        start: u64,
        end: u64,
    ) -> Result<StreamBlocksInRange<'_>, BlockStoreError> {
        let cf_blocks = self.cf(CF_BLOCKS)?;
        if start > end {
            return Ok(StreamBlocksInRange {
                store: self,
                pairs: Vec::new(),
                idx: 0,
                read_opts: self.blocks_stream_read_options(),
                cf_blocks,
            });
        }
        let cf_canon = self.cf(CF_CANONICAL)?;
        let mut ro_canon = ReadOptions::default();
        ro_canon.set_readahead_size(self.readahead_size);
        ro_canon.set_iterate_lower_bound(height_key(start).to_vec());
        if end < u64::MAX {
            ro_canon.set_iterate_upper_bound(height_key(end.saturating_add(1)).to_vec());
        }
        let iter = self.db.iterator_cf_opt(
            cf_canon,
            ro_canon,
            IteratorMode::From(height_key(start).as_slice(), Direction::Forward),
        );
        let mut pairs = Vec::new();
        for item in iter {
            let (k, v) = item?;
            let karr: [u8; 8] = k.as_ref().try_into().map_err(|_| {
                BlockStoreError::Serialization(
                    "stream_blocks_in_range: CF_CANONICAL key must be exactly 8 bytes".into(),
                )
            })?;
            let height = decode_height_key(&karr);
            if height > end {
                break;
            }
            if height < start {
                continue;
            }
            let varr: [u8; 32] = v.as_ref().try_into().map_err(|_| {
                BlockStoreError::Serialization(
                    "stream_blocks_in_range: CF_CANONICAL value must be exactly 32 bytes".into(),
                )
            })?;
            pairs.push((height, Bytes32::new(varr)));
        }
        Ok(StreamBlocksInRange {
            store: self,
            pairs,
            idx: 0,
            read_opts: self.blocks_stream_read_options(),
            cf_blocks,
        })
    }

    /// Retrieve a block header by hash ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md)).
    ///
    /// **Order:** [`Self::header_cache`] → on miss, `get_cf` [`CF_HEADERS`] → [`Self::deserialize_header`] (**no zstd**;
    /// [`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
    ///
    /// **Write path:** [`Self::put_block`] / [`Self::init_genesis`] insert headers in parallel with block bodies.
    pub fn get_header(&self, hash: &Bytes32) -> Result<Option<L2BlockHeader>, BlockStoreError> {
        if let Some(header) = self.header_cache.get_clone(hash) {
            return Ok(Some(header));
        }
        let cf = self.cf(CF_HEADERS)?;
        self.cf_headers_physical_gets
            .fetch_add(1, Ordering::Relaxed);
        let raw_opt = self.db.get_cf(cf, hash_key(hash).as_slice())?;
        let Some(raw) = raw_opt else {
            return Ok(None);
        };
        let header = Self::deserialize_header(&raw)?;
        self.header_cache.insert(*hash, header.clone());
        Ok(Some(header))
    }

    /// Drop one header from the in-memory LRU ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md) tests / future invalidation).
    pub fn invalidate_header_cache_entry(&self, hash: &Bytes32) {
        self.header_cache.remove(hash);
    }

    /// Count of RocksDB [`CF_HEADERS`] `get_cf` calls from [`Self::get_header`] or [`Self::get_record`]
    /// when the in-memory header **and** record caches do not already supply the header/record ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md), [`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md)).
    ///
    /// **Note:** [`Self::get_record`] consults [`Self::header_cache`] before touching RocksDB, so a record-cache
    /// miss with a warm header cache does **not** increment this counter (still satisfies “derive from header”).
    pub fn cf_headers_physical_get_count(&self) -> u64 {
        self.cf_headers_physical_gets.load(Ordering::Relaxed) as u64
    }

    /// **[`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md)** — Primary name in
    /// [`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 5.
    ///
    /// **Pipeline:** zstd payload → [`CF_BLOCKS`], bincode header → [`CF_HEADERS`], optional height index →
    /// [`CF_CANONICAL`]; [`BlockRecord`] is derived with [`BlockStatus::Validated`] and stored only in
    /// [`Self::record_cache`] ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) persistence rule).
    ///
    /// **Idempotency:** If the block hash already exists in `CF_BLOCKS`, returns `Ok(false)` and performs no writes
    /// ([`start.md`](../docs/prompt/start.md) hard requirement §9).
    ///
    /// **[`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md):** A successful insert that makes
    /// [`Self::block_count`] reach [`DICT_TRAINING_THRESHOLD`] triggers **one-time** dictionary training when
    /// [`BlockStoreConfig::use_compression_dict`](crate::BlockStoreConfig) is `true`.
    pub fn put_block(&self, block: &L2Block, canonical: bool) -> Result<bool, BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let hash = block.hash();
        let cf_b = self.cf(CF_BLOCKS)?;
        if self.db.get_cf(cf_b, hash_key(&hash).as_slice())?.is_some() {
            return Ok(false);
        }
        let compressed = self.serialize_block(block)?;
        let header_bytes = Self::serialize_header(&block.header)?;
        let mut batch = WriteBatch::default();
        let cf_h = self.cf(CF_HEADERS)?;
        batch.put_cf(cf_b, hash_key(&hash).as_slice(), &compressed);
        batch.put_cf(cf_h, hash_key(&hash).as_slice(), &header_bytes);
        if canonical {
            let cf_c = self.cf(CF_CANONICAL)?;
            batch.put_cf(cf_c, height_key(block.height()), hash_key(&hash).as_slice());
        }
        self.db.write(batch)?;
        if canonical {
            self.canonical_bin
                .write()
                .extend_write(block.height(), &hash)?;
        }
        let record = BlockRecord::from_header(&block.header, BlockStatus::Validated);
        self.record_cache.lock().insert(hash, record);
        self.block_cache.insert(hash, block.clone());
        self.header_cache.insert(hash, block.header.clone());
        self.maybe_train_dictionary()?;
        Ok(true)
    }

    /// Alias for [`Self::put_block`] — matches the BLK-001 normative snippet name `put` ([`NORMATIVE.md` § BLK-001](../docs/requirements/domains/block_storage/NORMATIVE.md)).
    #[inline]
    pub fn put(&self, block: &L2Block, canonical: bool) -> Result<bool, BlockStoreError> {
        self.put_block(block, canonical)
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

    /// **[`BLK-009`](../docs/requirements/domains/block_storage/specs/BLK-009.md)** — Persist an [`AttestedBlock`] under the block’s hash key.
    ///
    /// **Key:** [`hash_key`](crate::encoding::hash_key)(`hash`) — raw 32 bytes in [`CF_ATTESTED`] ([`KEY-001`](../docs/requirements/domains/key_encoding/specs/KEY-001_hash_keys.md)), identical key shape to [`CF_BLOCKS`] / [`CF_HEADERS`].
    ///
    /// **Value:** [`bincode::serialize`] of `attested` (uncompressed; attestations are small per BLK-009 implementation notes).
    ///
    /// **Hash vs payload:** Callers normally pass `hash == attested.hash()`; this method does **not** verify that invariant so
    /// tests and migration tooling can stage rows independently of body presence in [`CF_BLOCKS`].
    ///
    /// **Overwrite (AC §4):** A second call with the same `hash` replaces the previous value (`DB::put_cf`).
    ///
    /// **Read-only:** Returns [`BlockStoreError::Serialization`] with [`ERR_MUTATION_READ_ONLY`] — same contract as [`Self::put_block`].
    pub fn put_attestation(
        &self,
        hash: &Bytes32,
        attested: &AttestedBlock,
    ) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let bytes = bincode::serialize(attested)
            .map_err(|e| BlockStoreError::Serialization(e.to_string()))?;
        let cf = self.cf(CF_ATTESTED)?;
        self.db.put_cf(cf, hash_key(hash).as_slice(), &bytes)?;
        Ok(())
    }

    /// **[`BLK-009`](../docs/requirements/domains/block_storage/specs/BLK-009.md)** — Read [`AttestedBlock`] bytes from [`CF_ATTESTED`].
    ///
    /// **Miss:** [`Ok(None)]` when no row exists (AC §3).
    ///
    /// **No attestation cache (yet):** Each call performs a RocksDB `get_cf` + bincode decode (BLK-009 notes; hot paths may add [`CAC-*`] later).
    ///
    /// **Corrupt rows:** Malformed bincode surfaces as [`BlockStoreError::Serialization`] so operators can distinguish “missing” vs “bad bytes”.
    pub fn get_attestation(
        &self,
        hash: &Bytes32,
    ) -> Result<Option<AttestedBlock>, BlockStoreError> {
        let cf = self.cf(CF_ATTESTED)?;
        let raw = match self.db.get_cf(cf, hash_key(hash).as_slice())? {
            Some(b) => b,
            None => return Ok(None),
        };
        let attested: AttestedBlock = bincode::deserialize(&raw).map_err(|e| {
            BlockStoreError::Serialization(format!(
                "get_attestation: bincode deserialize failed: {e}"
            ))
        })?;
        Ok(Some(attested))
    }

    /// Async batched ingest ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md), [`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 5).
    ///
    /// **Channel + batching (NORMATIVE §1–3):** Enqueues into a bounded [`mpsc`] queue; a background task accumulates
    /// up to [`BlockStoreInner::pipeline_batch_size`] jobs or until [`BlockStoreInner::pipeline_flush_ms`] elapses,
    /// then applies **one** [`WriteBatch`] mirroring [`Self::put_block`] semantics.
    ///
    /// **Per-block ack ([`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md)):** The returned
    /// [`oneshot::Receiver`] resolves to the same `Result<bool, BlockStoreError>` shape as [`Self::put_block`]
    /// (`Ok(true)` inserted, `Ok(false)` duplicate).
    ///
    /// **Runtime contract:** The first call lazily spawns [`run_write_pipeline`] via [`tokio::spawn`]; therefore an
    /// active [`tokio::runtime::Handle`] must exist (integration tests should use `#[tokio::test]`).
    pub async fn put_pipelined(
        &self,
        block: L2Block,
        canonical: bool,
    ) -> Result<oneshot::Receiver<Result<bool, BlockStoreError>>, BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let tx = self.pipeline_sender().await?;
        let (ack_tx, ack_rx) = oneshot::channel();
        tx.send((block, canonical, ack_tx))
            .await
            .map_err(|_| BlockStoreError::PipelineClosed)?;
        Ok(ack_rx)
    }

    /// Count of successful RocksDB [`WriteBatch`] commits executed by the [`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) worker.
    ///
    /// **Instrumentation:** Used by `tests/blk_008_tests.rs` to prove AC §4 “single `WriteBatch` per flush interval”.
    #[must_use]
    pub fn pipeline_write_batch_count(&self) -> u64 {
        self.pipeline_write_batches.load(Ordering::Relaxed) as u64
    }

    /// Lazily constructs the bounded [`mpsc`] sender and spawns [`run_write_pipeline`].
    async fn pipeline_sender(&self) -> Result<mpsc::Sender<PipelineJob>, BlockStoreError> {
        let mut guard = self.pipeline_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            return Ok(tx.clone());
        }
        let _handle = tokio::runtime::Handle::try_current().map_err(|_| {
            BlockStoreError::Serialization(
                "put_pipelined requires an active Tokio runtime (use #[tokio::test] or Runtime::block_on)"
                    .into(),
            )
        })?;
        let cap = self.pipeline_channel_capacity;
        let (tx, rx) = mpsc::channel::<PipelineJob>(cap);
        let inner = self.inner.clone();
        let batch = self.pipeline_batch_size;
        let flush_ms = self.pipeline_flush_ms;
        tokio::spawn(run_write_pipeline(
            inner,
            Arc::new(tokio::sync::Mutex::new(None)),
            rx,
            batch,
            flush_ms,
        ));
        *guard = Some(tx.clone());
        Ok(tx)
    }

    /// Look up [`BlockRecord`] by hash ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md)).
    ///
    /// **Order**
    /// 1. [`Self::record_cache`] (Mutex map) — clone on hit; **no** RocksDB I/O.
    /// 2. [`Self::header_cache`] — if the header is already deserialized (e.g. after [`Self::put_block`] or
    ///    [`Self::get_header`]), derive [`BlockRecord::from_header`] with [`BlockStatus::Validated`] and insert into
    ///    the record cache; **no** RocksDB `get_cf` on [`CF_HEADERS`].
    /// 3. Else load raw bytes from [`CF_HEADERS`], increment [`Self::cf_headers_physical_gets`], deserialize via
    ///    [`Self::deserialize_header`], warm [`Self::header_cache`] + record cache.
    ///
    /// **Persistence:** [`BlockRecord`] is never written to any column family ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md)); only headers live under [`CF_HEADERS`].
    ///
    /// **Read-only stores:** Record/header RAM caches start empty; the first lookup may read [`CF_HEADERS`] and populate both caches without mutating on-disk layout beyond normal reads.
    pub fn get_record(&self, hash: &Bytes32) -> Result<Option<BlockRecord>, BlockStoreError> {
        {
            let guard = self.record_cache.lock();
            if let Some(r) = guard.get(hash) {
                return Ok(Some(r.clone()));
            }
        }
        if let Some(header) = self.header_cache.get_clone(hash) {
            let record = BlockRecord::from_header(&header, BlockStatus::Validated);
            self.record_cache.lock().insert(*hash, record.clone());
            return Ok(Some(record));
        }
        let cf = self.cf(CF_HEADERS)?;
        self.cf_headers_physical_gets
            .fetch_add(1, Ordering::Relaxed);
        let Some(bytes) = self.db.get_cf(cf, hash_key(hash).as_slice())? else {
            return Ok(None);
        };
        let header = Self::deserialize_header(&bytes)?;
        self.header_cache.insert(*hash, header.clone());
        let record = BlockRecord::from_header(&header, BlockStatus::Validated);
        self.record_cache.lock().insert(*hash, record.clone());
        Ok(Some(record))
    }

    /// Remove one hash from the in-memory [`BlockRecord`] map — **no RocksDB writes** ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md) test plan: simulate record-cache eviction).
    pub fn invalidate_record_cache_entry(&self, hash: &Bytes32) {
        let mut guard = self.record_cache.lock();
        let _ = guard.remove(hash);
    }

    /// **[`BLK-010`](../docs/requirements/domains/block_storage/specs/BLK-010.md)** — Set [`BlockRecord::status`] for a hash already present in [`Self::record_cache`].
    ///
    /// **No disk I/O:** [`BlockRecord`] is cache-only ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md)); this method never touches [`rocksdb::WriteBatch`] or [`DB::put_cf`](rocksdb::DB::put_cf).
    ///
    /// **`in_canonical_chain`:** Recomputed from [`BlockStatus::is_canonical`](dig_block::BlockStatus::is_canonical) so the row stays aligned with [`BlockRecord::from_header`](crate::types::BlockRecord::from_header) ([`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md) module docs).
    ///
    /// **Precondition:** The hash must already exist in the record cache (typically after [`Self::put_block`] or a cache-warming [`Self::get_record`]). Otherwise returns [`BlockStoreError::Serialization`] whose message starts with
    /// [`ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX`](crate::error::ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX)
    /// ([`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) caps the public enum at thirteen variants, so this uses the same stable-prefix pattern as read-only mutation guards).
    pub fn update_status(
        &self,
        hash: &Bytes32,
        status: BlockStatus,
    ) -> Result<(), BlockStoreError> {
        let mut guard = self.record_cache.lock();
        let record = guard.get_mut(hash).ok_or_else(|| {
            BlockStoreError::Serialization(format!(
                "{ERR_UPDATE_STATUS_RECORD_NOT_CACHED_PREFIX}{hash}"
            ))
        })?;
        record.status = status;
        record.in_canonical_chain = status.is_canonical();
        Ok(())
    }

    /// Count blocks persisted in [`CF_BLOCKS`] ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) threshold).
    ///
    /// **Operational note:** Implemented as a full-column scan — acceptable for the **rare** dictionary-training edge;
    /// production tipping paths should eventually cache this in [`CF_METADATA`](crate::CF_METADATA) if needed.
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
    fn sample_block_bodies(&self, sample_count: usize) -> Result<Vec<Vec<u8>>, BlockStoreError> {
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
    fn train_dictionary(&self) -> Result<Vec<u8>, BlockStoreError> {
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
    fn maybe_train_dictionary(&self) -> Result<(), BlockStoreError> {
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

    /// Async retrieval by hash ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md) AC §1, §4, §5).
    ///
    /// **Hot path:** [`Self::block_cache`] hits return cloned blocks **before** any `.await`, so the generated
    /// future can complete as [`Poll::Ready`] on the first [`poll`](std::future::Future::poll) without scheduling
    /// [`tokio::task::spawn_blocking`] (NORMATIVE BLK-007 §1–2).
    ///
    /// **Cold path:** Delegates to [`Self::get_block`] on the blocking pool so RocksDB + zstd never run on a
    /// cooperative tokio worker thread.
    pub async fn get_block_async(
        &self,
        hash: &Bytes32,
    ) -> Result<Option<L2Block>, BlockStoreError> {
        if let Some(block) = self.block_cache.get_clone(hash) {
            return Ok(Some(block));
        }
        let store = self.clone();
        let hash = *hash;
        tokio::task::spawn_blocking(move || store.get_block(&hash))
            .await
            .map_err(Self::map_spawn_join)?
    }

    /// Async header retrieval ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md) AC §2, §4–5).
    pub async fn get_header_async(
        &self,
        hash: &Bytes32,
    ) -> Result<Option<L2BlockHeader>, BlockStoreError> {
        if let Some(header) = self.header_cache.get_clone(hash) {
            return Ok(Some(header));
        }
        let store = self.clone();
        let hash = *hash;
        tokio::task::spawn_blocking(move || store.get_header(&hash))
            .await
            .map_err(Self::map_spawn_join)?
    }

    /// Async canonical-height lookup followed by block load ([`BLK-007`](../docs/requirements/domains/block_storage/specs/BLK-007.md) AC §3).
    ///
    /// **Always `spawn_blocking`:** height→hash uses [`CF_CANONICAL`] I/O; per BLK-007 implementation notes this
    /// stays on the blocking pool even when the block body would hit [`Self::block_cache`], avoiding partial
    /// “async hits” that still touch RocksDB in the sync prelude.
    pub async fn get_block_by_height_async(
        &self,
        height: u64,
    ) -> Result<Option<L2Block>, BlockStoreError> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_block_by_height(height))
            .await
            .map_err(Self::map_spawn_join)?
    }

    /// Maps a failed [`tokio::task::spawn_blocking`] join handle onto [`BlockStoreError::Serialization`].
    ///
    /// **Why not a dedicated enum variant:** [`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) caps
    /// the error surface at thirteen variants; join failures are rare operational faults surfaced with [`ERR_ASYNC_JOIN_PREFIX`].
    #[inline]
    fn map_spawn_join(err: tokio::task::JoinError) -> BlockStoreError {
        BlockStoreError::Serialization(format!("{ERR_ASYNC_JOIN_PREFIX}{err}"))
    }

    /// Resolve a column family handle by name, or error if the DB was not opened with it.
    ///
    /// This is a thin wrapper around [`DB::cf_handle`](rocksdb::DB::cf_handle) that converts
    /// the `Option<&ColumnFamily>` to our error type. In practice this should never fail
    /// because [`BlockStore::open`] creates all six families via [`cf_options::column_family_descriptors`],
    /// but defensive coding prevents silent `None` dereferences if the CF list drifts.
    fn cf(&self, name: &'static str) -> Result<&rocksdb::ColumnFamily, BlockStoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| BlockStoreError::Serialization(format!("missing column family {name}")))
    }
}

/// Background loop draining [`PipelineJob`] values into batched [`WriteBatch`] commits ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)).
///
/// **Shutdown (AC §8):** Ingress senders live on [`BlockStore::pipeline_tx`], not on [`BlockStoreInner`]. When the last
/// [`BlockStore`] clone is dropped, the final [`mpsc::Sender`] is released, `rx.recv()` yields `None`, and we
/// [`flush_pipeline_batch`] any tail buffer before exiting.
async fn run_write_pipeline(
    inner: Arc<BlockStoreInner>,
    _worker_unused_pipeline_tx: Arc<tokio::sync::Mutex<Option<mpsc::Sender<PipelineJob>>>>,
    mut rx: mpsc::Receiver<PipelineJob>,
    batch_size: usize,
    flush_ms: u64,
) {
    // The real ingress [`mpsc::Sender`] lives on user-facing [`BlockStore`] handles; this placeholder exists only so
    // we can reuse [`BlockStore`] / [`Deref`] helpers inside [`flush_pipeline_batch`] without threading a parallel
    // “inner-only” facade type through the module.
    let store = BlockStore {
        inner,
        pipeline_tx: _worker_unused_pipeline_tx,
    };
    let mut buf: Vec<PipelineJob> = Vec::with_capacity(batch_size);
    let tick = Duration::from_millis(flush_ms);

    loop {
        match rx.recv().await {
            None => {
                return;
            }
            Some(job) => buf.push(job),
        }
        if buf.len() >= batch_size {
            let _ = flush_pipeline_batch(&store, &mut buf);
            buf.clear();
            continue;
        }

        let mut sleep = Box::pin(tokio::time::sleep(tick));
        'collect: loop {
            tokio::select! {
                biased;
                maybe = rx.recv() => {
                    match maybe {
                        None => {
                            let _ = flush_pipeline_batch(&store, &mut buf);
                            return;
                        }
                        Some(job) => {
                            buf.push(job);
                            if buf.len() >= batch_size {
                                break 'collect;
                            }
                        }
                    }
                }
                _ = &mut sleep, if !buf.is_empty() => {
                    break 'collect;
                }
            }
        }

        let _ = flush_pipeline_batch(&store, &mut buf);
        buf.clear();
    }
}

/// Applies one RocksDB [`WriteBatch`] for all novel inserts in `jobs`, mirroring [`BlockStore::put_block`].
///
/// **Idempotency (AC §5):** Duplicate hashes already on disk **or** repeated within `jobs` are answered with
/// `Ok(false)` acks and are omitted from the write batch. A completely duplicate batch performs **no** `db.write`.
///
/// **Errors:** Build/IO failures notify every still-pending staged [`oneshot`] with [`BlockStoreError::Serialization`]
/// carrying the diagnostic string, then the function returns `Ok(())` so the worker loop keeps draining (best-effort
/// bulk ingest semantics; callers observe failure on their ack channel).
fn flush_pipeline_batch(
    store: &BlockStore,
    jobs: &mut Vec<PipelineJob>,
) -> Result<(), BlockStoreError> {
    if store.read_only {
        let pending: Vec<PipelineJob> = std::mem::take(jobs);
        let msg = ERR_MUTATION_READ_ONLY.to_string();
        for (_, _, ack) in pending {
            let _ = ack.send(Err(BlockStoreError::Serialization(msg.clone())));
        }
        return Ok(());
    }
    let pending: Vec<PipelineJob> = std::mem::take(jobs);
    let cf_b = match store.cf(CF_BLOCKS) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };
    let cf_h = match store.cf(CF_HEADERS) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };
    let cf_c = match store.cf(CF_CANONICAL) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };

    let mut seen: HashSet<Bytes32> = HashSet::new();
    struct StagedRow {
        hash: Bytes32,
        block: L2Block,
        compressed: Vec<u8>,
        header_bytes: Vec<u8>,
        canonical: bool,
        ack: oneshot::Sender<Result<bool, BlockStoreError>>,
    }
    let mut staged: Vec<StagedRow> = Vec::new();

    for (block, canonical, ack) in pending {
        let hash = block.hash();
        if !seen.insert(hash) {
            let _ = ack.send(Ok(false));
            continue;
        }
        let exists = match store.db.get_cf(cf_b, hash_key(&hash).as_slice()) {
            Ok(o) => o.is_some(),
            Err(e) => {
                let _ = ack.send(Err(BlockStoreError::RocksDb(e)));
                continue;
            }
        };
        if exists {
            let _ = ack.send(Ok(false));
            continue;
        }
        let compressed = match store.serialize_block(&block) {
            Ok(b) => b,
            Err(e) => {
                let _ = ack.send(Err(e));
                continue;
            }
        };
        let header_bytes = match BlockStore::serialize_header(&block.header) {
            Ok(b) => b,
            Err(e) => {
                let _ = ack.send(Err(e));
                continue;
            }
        };
        staged.push(StagedRow {
            hash,
            block,
            compressed,
            header_bytes,
            canonical,
            ack,
        });
    }

    let mut wb = WriteBatch::default();
    for row in &staged {
        wb.put_cf(
            cf_b,
            hash_key(&row.hash).as_slice(),
            row.compressed.as_slice(),
        );
        wb.put_cf(
            cf_h,
            hash_key(&row.hash).as_slice(),
            row.header_bytes.as_slice(),
        );
        if row.canonical {
            wb.put_cf(
                cf_c,
                height_key(row.block.height()),
                hash_key(&row.hash).as_slice(),
            );
        }
    }

    if wb.is_empty() {
        return Ok(());
    }

    if let Err(e) = store.db.write(wb) {
        let msg = format!("write pipeline: rocksdb write failed: {e}");
        for row in staged {
            let _ = row
                .ack
                .send(Err(BlockStoreError::Serialization(msg.clone())));
        }
        return Ok(());
    }

    for row in &staged {
        if row.canonical {
            if let Err(e) = store
                .canonical_bin
                .write()
                .extend_write(row.block.height(), &row.hash)
            {
                let msg = format!("write pipeline: canonical.bin mmap update failed: {e}");
                for row in staged {
                    let _ = row
                        .ack
                        .send(Err(BlockStoreError::Serialization(msg.clone())));
                }
                return Ok(());
            }
        }
    }

    store.pipeline_write_batches.fetch_add(1, Ordering::Relaxed);

    for row in staged {
        let record = BlockRecord::from_header(&row.block.header, BlockStatus::Validated);
        store.record_cache.lock().insert(row.hash, record);
        store.block_cache.insert(row.hash, row.block.clone());
        store
            .header_cache
            .insert(row.hash, row.block.header.clone());
        let ack_res = match store.maybe_train_dictionary() {
            Ok(()) => Ok(true),
            Err(e) => Err(e),
        };
        let _ = row.ack.send(ack_res);
    }
    Ok(())
}

/// Lazy iterator over canonical block bodies for a closed height range ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).
///
/// Constructed only via [`BlockStore::stream_blocks_in_range`]. Holds a precomputed `(height, hash)` list from a
/// readahead-backed scan of [`CF_CANONICAL`], then loads [`CF_BLOCKS`] rows on demand so callers can stop early without
/// decompressing the remainder ([`BLK-006.md`](../docs/requirements/domains/block_storage/specs/BLK-006.md) implementation notes).
///
/// **Invariants:** Heights in `pairs` are strictly ascending (RocksDB canonical ordering). Each successful item matches
/// the canonical hash at that height; [`L2Block::height`](dig_block::L2Block::height) should equal the stored height
/// when the database is consistent ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md) write path).
pub struct StreamBlocksInRange<'a> {
    store: &'a BlockStore,
    pairs: Vec<(u64, Bytes32)>,
    idx: usize,
    read_opts: ReadOptions,
    cf_blocks: &'a ColumnFamily,
}

impl<'a> Iterator for StreamBlocksInRange<'a> {
    type Item = Result<L2Block, BlockStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.pairs.len() {
            return None;
        }
        let (_expected_height, hash) = self.pairs[self.idx];
        self.idx += 1;
        if let Some(block) = self.store.block_cache.get_clone(&hash) {
            return Some(Ok(block));
        }
        self.store
            .cf_blocks_stream_physical_gets
            .fetch_add(1, Ordering::Relaxed);
        let raw_opt = match self.store.db.get_cf_opt(
            self.cf_blocks,
            hash_key(&hash).as_slice(),
            &self.read_opts,
        ) {
            Ok(o) => o,
            Err(e) => return Some(Err(e.into())),
        };
        let Some(raw) = raw_opt else {
            return Some(Err(BlockStoreError::BlockNotFound(hash)));
        };
        match self.store.deserialize_block(&raw) {
            Ok(block) => {
                self.store.block_cache.insert(hash, block.clone());
                self.store.header_cache.insert(hash, block.header.clone());
                Some(Ok(block))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Determine the initial zstd dictionary for a freshly opened store.
///
/// # Priority order
///
/// 1. **Config override** (`zstd_dictionary_override`) — used in tests ([`SER-001`] / [`SER-005`])
///    to inject a pre-trained dictionary without persisting it in [`META_ZSTD_DICT`].
///    Empty override bytes are treated as "no dictionary" (returns `None`).
/// 2. **Persisted metadata** — calls [`load_zstd_dict_from_db`] to check [`CF_METADATA`]
///    for a non-empty [`META_ZSTD_DICT`] row written by [`BlockStore::train_dictionary`].
/// 3. **None** — no dictionary available yet; compression falls through to plain zstd.
///
/// # Called by
///
/// [`BlockStore::open`] and [`BlockStore::open_readonly`] during construction.
fn resolve_zstd_dictionary(
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
fn load_zstd_dict_from_db(
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

/// Load the current chain tip from [`CF_METADATA`] / [`META_TIP`].
///
/// The tip is a 40-byte value encoding `hash (32 bytes) || height (8 bytes LE)`,
/// decoded via [`ChainTip::from_bytes`]. Returns `None` for a brand-new database
/// that has not yet had [`BlockStore::init_genesis`] called.
///
/// # Chia analogy
///
/// Corresponds to reading `current_peak` from the `block_store` metadata in Chia's
/// `BlockStore.get_peak()`. The DIG version uses a fixed-width binary encoding
/// instead of SQLite row access.
///
/// # Called by
///
/// [`BlockStore::open`] and [`BlockStore::open_readonly`] to populate the in-memory
/// [`BlockStore::tip`] field at startup.
fn load_tip(db: &DB) -> Result<Option<ChainTip>, BlockStoreError> {
    let meta = db
        .cf_handle(CF_METADATA)
        .ok_or_else(|| BlockStoreError::Serialization("missing CF_METADATA".into()))?;
    let Some(raw) = db.get_cf(meta, META_TIP.as_bytes())? else {
        return Ok(None);
    };
    ChainTip::from_bytes(&raw).map(Some)
}

/// Pre-verify recent canonical blocks exist in [`CF_BLOCKS`] during startup warming.
///
/// Walks backward from `tip.height` for `depth` heights, reading each height's hash
/// from [`CF_CANONICAL`] and probing [`CF_BLOCKS`] for existence. This does **not**
/// deserialize blocks or populate the in-memory LRU (that happens lazily on first
/// `get_block` call); it only counts how many blocks are physically present.
///
/// # Chia analogy
///
/// Chia's `BlockStore` does not warm caches on startup in the same way, but the
/// concept maps to the `_load_block_records` path that pre-populates the
/// `block_record_cache` from SQLite. DIG's approach is lighter: we only verify
/// existence, deferring deserialization to first access.
///
/// # Parameters
///
/// - `db` — RocksDB handle (not yet wrapped in `BlockStore`; called during construction).
/// - `tip` — current chain tip; if `None`, returns 0 immediately (no genesis yet).
/// - `depth` — number of trailing heights to probe, configured via
///   [`BlockStoreConfig::warm_cache_depth`](crate::BlockStoreConfig::warm_cache_depth).
///
/// # Returns
///
/// Count of heights where a block body was found in [`CF_BLOCKS`]. Exposed via
/// [`BlockStore::warm_blocks_loaded_count`] for startup diagnostics and test assertions.
///
/// # Called by
///
/// [`BlockStore::open`] when [`BlockStoreConfig::warm_cache_on_open`](crate::BlockStoreConfig::warm_cache_on_open)
/// is `true` ([`CAC-006`](../docs/requirements/domains/caching/specs/CAC-006_cache_warming_on_startup.md)).
fn warm_recent_blocks(
    db: &DB,
    tip: &Option<ChainTip>,
    depth: u64,
) -> Result<usize, BlockStoreError> {
    let Some(t) = tip else {
        return Ok(0);
    };
    let cf_c = db
        .cf_handle(CF_CANONICAL)
        .ok_or_else(|| BlockStoreError::Serialization("missing CF_CANONICAL".into()))?;
    let cf_b = db
        .cf_handle(CF_BLOCKS)
        .ok_or_else(|| BlockStoreError::Serialization("missing CF_BLOCKS".into()))?;
    let mut count = 0usize;
    let start = t.height.saturating_sub(depth.saturating_sub(1));
    for h in start..=t.height {
        let key = height_key(h);
        let Some(hash_bytes) = db.get_cf(cf_c, key)? else {
            continue;
        };
        if hash_bytes.len() != 32 {
            continue;
        }
        let arr: [u8; 32] = hash_bytes
            .as_slice()
            .try_into()
            .map_err(|_| BlockStoreError::Serialization("canonical entry not 32 bytes".into()))?;
        let hash = Bytes32::new(arr);
        if db.get_cf(cf_b, hash_key(&hash).as_slice())?.is_some() {
            count += 1;
        }
    }
    Ok(count)
}
