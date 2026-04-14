//! `BlockStore` — RocksDB-backed persistent block and chain state.
//!
//! **Architecture**
//! - Owns column families in [`crate::constants`] ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
//! - Block bodies: `bincode` + zstd ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)); headers: `bincode` only ([`SER-002`](../docs/requirements/domains/serialization/specs/SER-002.md)).
//! - Tip / genesis metadata: [`crate::constants::META_TIP`], [`crate::constants::META_GENESIS_HASH`].
//!
//! **Spec:** `docs/resources/SPEC.md` §15.1 (constructors), §16 (crate boundary).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chia_protocol::Bytes32;
use dig_block::L2Block;
use parking_lot::RwLock;
use rocksdb::{Options, WriteBatch, DB};

use crate::cf_options;
use crate::constants::{
    CF_BLOCKS, CF_CANONICAL, CF_HEADERS, CF_METADATA, META_GENESIS_HASH, META_TIP, META_ZSTD_DICT,
};
use crate::encoding::{hash_key, height_key};
use crate::error::{
    BlockStoreError, ERR_INIT_GENESIS_ALREADY_INITIALIZED, ERR_INIT_GENESIS_READ_ONLY,
    ERR_OPEN_READONLY_PATH_MISSING_PREFIX,
};
use crate::types::ChainTip;
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
    zstd_dict: Option<Arc<Vec<u8>>>,
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
        Ok(Self {
            db,
            read_only: false,
            tip: RwLock::new(tip),
            warm_blocks_loaded: AtomicUsize::new(warm),
            compression_level,
            use_compression_dict,
            max_decompressed_block_bytes,
            zstd_dict,
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
        Ok(Self {
            db,
            read_only: true,
            tip: RwLock::new(tip),
            warm_blocks_loaded: AtomicUsize::new(0),
            compression_level,
            use_compression_dict,
            max_decompressed_block_bytes,
            zstd_dict,
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
        let header_bytes = bincode::serialize(&block.header)?;
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

    /// Serialize then zstd-compress a block for [`CF_BLOCKS`] ([`SER-001`](../docs/requirements/domains/serialization/specs/SER-001.md)).
    ///
    /// **Pipeline:** [`bincode::serialize`] → [`zstd::bulk::Compressor::with_dictionary`] when
    /// [`Self::use_compression_dict`] and a dictionary are present; otherwise [`zstd::encode_all`] (plain zstd).
    ///
    /// **Errors:** [`BlockStoreError::Serialization`] from bincode; [`BlockStoreError::Compression`] from zstd.
    pub fn serialize_block(&self, block: &L2Block) -> Result<Vec<u8>, BlockStoreError> {
        let raw = bincode::serialize(block)?;
        if self.use_compression_dict {
            if let Some(dict) = &self.zstd_dict {
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
            if let Some(dict) = &self.zstd_dict {
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

    /// Deserialize a full block by hash ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) precursor).
    pub fn get_block(&self, hash: &Bytes32) -> Result<Option<L2Block>, BlockStoreError> {
        let cf = self.cf(CF_BLOCKS)?;
        let Some(raw) = self.db.get_cf(cf, hash_key(hash).as_slice())? else {
            return Ok(None);
        };
        Ok(Some(self.deserialize_block(&raw)?))
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
