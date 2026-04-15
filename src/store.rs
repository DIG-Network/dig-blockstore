//! `BlockStore` — RocksDB-backed persistent block and chain state.
//!
//! **Architecture**
//! - Owns column families in [`crate::constants`] ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
//! - Block bodies: `bincode` + zstd ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)); headers: `bincode` only ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
//! - Tip / genesis metadata: [`crate::constants::META_TIP`], [`crate::constants::META_GENESIS_HASH`].
//! - [`BlockRecord`](crate::BlockRecord) cache: in-memory only ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md), [`TYP-004`](../docs/requirements/domains/storage_types/specs/TYP-004.md)).
//! - Block body cache: [`crate::cache::sharded::ShardedBlockCache`] ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md), [`CAC-001`](../docs/requirements/domains/caching/specs/CAC-001_sharded_block_cache.md)).
//! - Header cache: [`crate::cache::sharded::ShardedHeaderCache`] ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md), [`CAC-002`](../docs/requirements/domains/caching/specs/CAC-002_sharded_header_cache.md)).
//!
//! **Spec:** `docs/resources/SPEC.md` §15.1 (constructors), §16 (crate boundary).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chia_protocol::Bytes32;
use dig_block::{BlockStatus, L2Block, L2BlockHeader};
use parking_lot::{Mutex, RwLock};
use rand::seq::SliceRandom;
use rocksdb::{IteratorMode, Options, WriteBatch, DB};

use crate::cache::sharded::{ShardedBlockCache, ShardedHeaderCache};
use crate::cf_options;
use crate::constants::{
    CF_BLOCKS, CF_CANONICAL, CF_HEADERS, CF_METADATA, DICT_TARGET_SIZE, DICT_TRAINING_THRESHOLD,
    META_GENESIS_HASH, META_TIP, META_ZSTD_DICT,
};
use crate::encoding::{hash_key, height_key};
use crate::error::{
    BlockStoreError, ERR_INIT_GENESIS_ALREADY_INITIALIZED, ERR_INIT_GENESIS_READ_ONLY,
    ERR_MUTATION_READ_ONLY, ERR_OPEN_READONLY_PATH_MISSING_PREFIX,
};
use crate::types::{BlockRecord, ChainTip};
use crate::BlockStoreConfig;

/// Primary handle for all block persistence APIs.
pub struct BlockStore {
    db: Arc<DB>,
    read_only: bool,
    tip: RwLock<Option<ChainTip>>,
    warm_blocks_loaded: AtomicUsize,
    /// Zstd level for [`Self::serialize_block`] / plain fallback ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) §6).
    compression_level: i32,
    /// When true and [`Self::zstd_dict`] is [`Some`], compress with dictionary ([`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md) precursor).
    use_compression_dict: bool,
    /// Cap passed to [`zstd::bulk::Decompressor::decompress`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md) implementation notes).
    max_decompressed_block_bytes: usize,
    /// Trained dictionary loaded from [`META_ZSTD_DICT`] or [`BlockStoreConfig::zstd_dictionary_override`].
    ///
    /// **[`SER-005`](../docs/requirements/domains/serialization/specs/SER-005.md):** [`RwLock`] lets
    /// [`Self::maybe_train_dictionary`] publish the first trained dictionary **after** the write that crosses
    /// [`DICT_TRAINING_THRESHOLD`](crate::constants::DICT_TRAINING_THRESHOLD) while keeping [`BlockStore`] on an
    /// immutable `&self` API surface (matches `put`-style ergonomics slated for [`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md)).
    zstd_dict: RwLock<Option<Arc<Vec<u8>>>>,
    /// [`BlockRecord`] rows derived on write; **never** persisted ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md), [`CAC-003`](../docs/requirements/domains/caching/specs/CAC-003.md) precursor).
    ///
    /// **Concurrency:** [`parking_lot::Mutex`] keeps inserts from [`Self::put_block`] / [`Self::init_genesis`] and
    /// lookups from [`Self::get_record`] safe without `&mut self`.
    record_cache: Mutex<HashMap<Bytes32, BlockRecord>>,
    /// Sharded LRU of deserialized [`L2Block`] values ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md)).
    block_cache: Arc<ShardedBlockCache>,
    /// Count of RocksDB `get_cf` calls against [`CF_BLOCKS`] issued from [`Self::get_block`] **after** a cache miss.
    ///
    /// **Rationale:** Proves AC §2 “no I/O on hit” in `tests/blk_002_tests.rs`; cheap atomic hot path on miss only.
    cf_blocks_physical_gets: AtomicUsize,
    /// Sharded LRU of [`L2BlockHeader`] values ([`BLK-003`](../docs/requirements/domains/block_storage/specs/BLK-003.md)).
    ///
    /// **Separate** from [`Self::block_cache`] per BLK-003 implementation notes (tunables: [`BlockStoreConfig::header_cache_capacity`](crate::BlockStoreConfig::header_cache_capacity)).
    header_cache: Arc<ShardedHeaderCache>,
    /// Count of RocksDB `get_cf` calls against [`CF_HEADERS`] from [`Self::get_header`] after a header-cache miss.
    cf_headers_physical_gets: AtomicUsize,
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
        let zstd_dict =
            resolve_zstd_dictionary(&db, use_compression_dict, zstd_dictionary_override)?;
        let tip = load_tip(&db)?;
        let warm = if config.warm_cache_on_open {
            warm_recent_blocks(&db, &tip, config.warm_cache_depth)?
        } else {
            0
        };
        let shards = config.cache_shards.max(1);
        let block_cache = Arc::new(ShardedBlockCache::new(config.block_cache_capacity, shards));
        let header_cache = Arc::new(ShardedHeaderCache::new(
            config.header_cache_capacity,
            shards,
        ));
        Ok(Self {
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
            header_cache,
            cf_headers_physical_gets: AtomicUsize::new(0),
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
        let zstd_dict =
            resolve_zstd_dictionary(&db, use_compression_dict, zstd_dictionary_override)?;
        let tip = load_tip(&db)?;
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
            header_cache,
            cf_headers_physical_gets: AtomicUsize::new(0),
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
        *self.tip.write() = Some(tip);
        let record = BlockRecord::from_header(&block.header, BlockStatus::Validated);
        self.record_cache.lock().insert(hash, record);
        self.block_cache.insert(hash, block.clone());
        self.header_cache.insert(hash, block.header.clone());
        self.maybe_train_dictionary()?;
        Ok(())
    }

    /// Current chain tip loaded from metadata / in-memory cache ([`CAN-007`](../docs/requirements/domains/canonical_chain/specs/CAN-007.md) preview).
    pub fn tip(&self) -> Option<ChainTip> {
        *self.tip.read()
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

    fn decompress_block_payload(&self, compressed: &[u8]) -> std::io::Result<Vec<u8>> {
        if self.use_compression_dict {
            if let Some(dict) = self.zstd_dict.read().as_ref() {
                let mut decompressor = zstd::bulk::Decompressor::with_dictionary(dict.as_slice())?;
                return match decompressor.decompress(compressed, self.max_decompressed_block_bytes)
                {
                    Ok(bytes) => Ok(bytes),
                    Err(_) => zstd::decode_all(compressed),
                };
            }
        }
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
        self.cf_blocks_physical_gets
            .fetch_add(1, Ordering::Relaxed);
        let raw_opt = self.db.get_cf(cf, hash_key(hash).as_slice())?;
        let Some(raw) = raw_opt else {
            return Ok(None);
        };
        let block = self.deserialize_block(&raw)?;
        self.block_cache.insert(*hash, block.clone());
        self.header_cache.insert(*hash, block.header.clone());
        Ok(Some(block))
    }

    /// Drop a single entry from the in-memory block LRU — **no RocksDB writes** ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) test plan: simulate eviction).
    pub fn invalidate_block_cache_entry(&self, hash: &Bytes32) {
        self.block_cache.remove(hash);
    }

    /// How many times [`Self::get_block`] reached RocksDB [`CF_BLOCKS`] after a cache miss (includes `Ok(None)` probes).
    ///
    /// **Tests / ops:** [`tests/blk_002_tests.rs`] asserts hits add zero; misses increment exactly once per call.
    pub fn cf_blocks_physical_get_count(&self) -> u64 {
        self.cf_blocks_physical_gets.load(Ordering::Relaxed) as u64
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

    /// Count of RocksDB [`CF_HEADERS`] reads from [`Self::get_header`] after a header-cache miss.
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

    /// Look up [`BlockRecord`] by hash: **cache first**, then [`CF_HEADERS`] + [`BlockRecord::from_header`] on miss
    /// ([`BLK-004`](../docs/requirements/domains/block_storage/specs/BLK-004.md) will extend status / eviction policy).
    ///
    /// **Read-only stores:** Cache starts empty; the first hit populates from persisted headers without mutating RocksDB.
    pub fn get_record(&self, hash: &Bytes32) -> Result<Option<BlockRecord>, BlockStoreError> {
        {
            let guard = self.record_cache.lock();
            if let Some(r) = guard.get(hash) {
                return Ok(Some(r.clone()));
            }
        }
        let cf = self.cf(CF_HEADERS)?;
        let Some(bytes) = self.db.get_cf(cf, hash_key(hash).as_slice())? else {
            return Ok(None);
        };
        let header = Self::deserialize_header(&bytes)?;
        self.header_cache.insert(*hash, header.clone());
        let record = BlockRecord::from_header(&header, BlockStatus::Validated);
        let mut guard = self.record_cache.lock();
        guard.insert(*hash, record.clone());
        Ok(Some(record))
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

    fn cf(&self, name: &'static str) -> Result<&rocksdb::ColumnFamily, BlockStoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| BlockStoreError::Serialization(format!("missing column family {name}")))
    }
}

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

fn load_tip(db: &DB) -> Result<Option<ChainTip>, BlockStoreError> {
    let meta = db
        .cf_handle(CF_METADATA)
        .ok_or_else(|| BlockStoreError::Serialization("missing CF_METADATA".into()))?;
    let Some(raw) = db.get_cf(meta, META_TIP.as_bytes())? else {
        return Ok(None);
    };
    ChainTip::from_bytes(&raw).map(Some)
}

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
