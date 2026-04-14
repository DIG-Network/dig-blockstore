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
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};

use crate::constants::{
    ALL_COLUMN_FAMILIES, CF_BLOCKS, CF_CANONICAL, CF_HEADERS, CF_METADATA, META_GENESIS_HASH,
    META_TIP,
};
use crate::encoding::{hash_key, height_key};
use crate::error::BlockStoreError;
use crate::types::ChainTip;
use crate::BlockStoreConfig;

/// Primary handle for all block persistence APIs.
pub struct BlockStore {
    db: Arc<DB>,
    read_only: bool,
    tip: RwLock<Option<ChainTip>>,
    warm_blocks_loaded: AtomicUsize,
}

impl BlockStore {
    /// Open or create a store at `config.db_path` with all column families ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub fn open(config: BlockStoreConfig) -> Result<Self, BlockStoreError> {
        std::fs::create_dir_all(&config.db_path)?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        let cfs: Vec<_> = ALL_COLUMN_FAMILIES
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
            .collect();
        let db = DB::open_cf_descriptors(&opts, &config.db_path, cfs)?;
        let db = Arc::new(db);
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
        })
    }

    /// Open an existing database read-only; fails if `path` does not exist ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub fn open_readonly(path: impl AsRef<Path>) -> Result<Self, BlockStoreError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(BlockStoreError::PathDoesNotExist(path.to_path_buf()));
        }
        let opts = Options::default();
        let cfs: Vec<_> = ALL_COLUMN_FAMILIES
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
            .collect();
        let db = DB::open_cf_descriptors_read_only(&opts, path, cfs, false)?;
        let db = Arc::new(db);
        let tip = load_tip(&db)?;
        Ok(Self {
            db,
            read_only: true,
            tip: RwLock::new(tip),
            warm_blocks_loaded: AtomicUsize::new(0),
        })
    }

    /// Initialize genesis: empty store only; atomic [`WriteBatch`] ([`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md)).
    pub fn init_genesis(&self, block: &L2Block) -> Result<(), BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::ReadOnly);
        }
        let meta = self.cf(CF_METADATA)?;
        if self.db.get_cf(meta, META_TIP.as_bytes())?.is_some()
            || self
                .db
                .get_cf(meta, META_GENESIS_HASH.as_bytes())?
                .is_some()
        {
            return Err(BlockStoreError::AlreadyInitialized);
        }
        let hash = block.hash();
        if block.height() != 0 {
            return Err(BlockStoreError::InvalidData(format!(
                "genesis block height must be 0, got {}",
                block.height()
            )));
        }
        let block_bytes = bincode::serialize(block)
            .map_err(|e| BlockStoreError::Serialization(format!("bincode block: {e}")))?;
        let compressed = zstd::encode_all(block_bytes.as_slice(), 3)
            .map_err(|e| BlockStoreError::Zstd(e.to_string()))?;
        let header_bytes = bincode::serialize(&block.header)
            .map_err(|e| BlockStoreError::Serialization(format!("bincode header: {e}")))?;
        let tip = ChainTip { hash, height: 0 };
        let mut batch = WriteBatch::default();
        let cf_b = self.cf(CF_BLOCKS)?;
        let cf_h = self.cf(CF_HEADERS)?;
        let cf_c = self.cf(CF_CANONICAL)?;
        batch.put_cf(cf_b, hash_key(&hash).as_ref(), &compressed);
        batch.put_cf(cf_h, hash_key(&hash).as_ref(), &header_bytes);
        batch.put_cf(cf_c, height_key(0), hash_key(&hash).as_ref());
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

    /// Deserialize a full block by hash ([`BLK-002`](../docs/requirements/domains/block_storage/specs/BLK-002.md) precursor).
    pub fn get_block(&self, hash: &Bytes32) -> Result<Option<L2Block>, BlockStoreError> {
        let cf = self.cf(CF_BLOCKS)?;
        let Some(raw) = self.db.get_cf(cf, hash_key(hash).as_ref())? else {
            return Ok(None);
        };
        let decompressed =
            zstd::decode_all(raw.as_slice()).map_err(|e| BlockStoreError::Zstd(e.to_string()))?;
        let block: L2Block = bincode::deserialize(&decompressed)
            .map_err(|e| BlockStoreError::Serialization(format!("bincode block decode: {e}")))?;
        Ok(Some(block))
    }

    fn cf(&self, name: &'static str) -> Result<&rocksdb::ColumnFamily, BlockStoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| BlockStoreError::InvalidData(format!("missing column family {name}")))
    }
}

fn load_tip(db: &DB) -> Result<Option<ChainTip>, BlockStoreError> {
    let meta = db
        .cf_handle(CF_METADATA)
        .ok_or_else(|| BlockStoreError::InvalidData("missing CF_METADATA".into()))?;
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
        .ok_or_else(|| BlockStoreError::InvalidData("missing CF_CANONICAL".into()))?;
    let cf_b = db
        .cf_handle(CF_BLOCKS)
        .ok_or_else(|| BlockStoreError::InvalidData("missing CF_BLOCKS".into()))?;
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
            .map_err(|_| BlockStoreError::InvalidData("canonical entry not 32 bytes".into()))?;
        let hash = Bytes32::new(arr);
        if db.get_cf(cf_b, hash_key(&hash).as_ref())?.is_some() {
            count += 1;
        }
    }
    Ok(count)
}
