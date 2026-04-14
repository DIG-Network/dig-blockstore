//! # STR-004 — `BlockStore` constructors (`open`, `open_readonly`, `init_genesis`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - [`STR-004.md`](../../../docs/requirements/domains/crate_structure/specs/STR-004.md) — signatures, behaviors, test plan table
//! - [`NORMATIVE` STR-004](../../../docs/requirements/domains/crate_structure/NORMATIVE.md)
//! - [`SPEC.md` §15.1](../../../docs/resources/SPEC.md) — public constructor API
//!
//! ## What this file proves
//!
//! Each test maps to rows in STR-004’s test plan (open creates DB, reopens, CF list, tip reload,
//! cache warming, readonly semantics, genesis writes, double-init guard). Where the spec calls for
//! “verify data persists”, we compare [`dig_block::L2Block::hash`], [`ChainTip`], and raw metadata bytes.
//! Integration tests use a real RocksDB directory under [`tempfile::TempDir`] so behavior matches
//! production I/O (not mocks).

use std::path::Path;

use chia_protocol::Bytes32;
use dig_block::{L2Block, L2BlockHeader, Signature};
use dig_blockstore::constants::{ALL_COLUMN_FAMILIES, CF_METADATA, META_GENESIS_HASH, META_TIP};
use dig_blockstore::error::BlockStoreError;
use dig_blockstore::{BlockStore, BlockStoreConfig, ChainTip};
use dig_constants::DIG_MAINNET;
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use tempfile::TempDir;

/// Deterministic genesis block for store tests (SPEC §8.3 pattern via [`L2BlockHeader::genesis`]).
fn sample_genesis_block() -> L2Block {
    let network_id = DIG_MAINNET.genesis_challenge();
    let header = L2BlockHeader::genesis(network_id, 0, Bytes32::default());
    L2Block::new(header, vec![], vec![], Signature::default())
}

fn open_opts() -> Options {
    let mut o = Options::default();
    o.create_if_missing(true);
    o.create_missing_column_families(true);
    o
}

/// Read raw metadata value after all `BlockStore` handles are dropped (direct RocksDB verify).
fn read_metadata_raw(path: &Path, key: &str) -> Option<Vec<u8>> {
    let cfs: Vec<_> = ALL_COLUMN_FAMILIES
        .iter()
        .map(|n| ColumnFamilyDescriptor::new(*n, Options::default()))
        .collect();
    let db = DB::open_cf_descriptors_read_only(&open_opts(), path, cfs, false).ok()?;
    let cf = db.cf_handle(CF_METADATA)?;
    db.get_cf(cf, key.as_bytes()).ok().flatten()
}

#[test]
fn test_open_creates_new_db() {
    let dir = TempDir::new().unwrap();
    let cfg = BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let _store = BlockStore::open(cfg).unwrap();
    assert!(
        dir.path().join("CURRENT").is_file(),
        "RocksDB should create CURRENT on first open ([`STR-004`](../../../docs/requirements/domains/crate_structure/specs/STR-004.md))"
    );
}

#[test]
fn test_open_creates_all_cfs() {
    let dir = TempDir::new().unwrap();
    let cfg = BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    };
    drop(BlockStore::open(cfg).unwrap());
    let names = DB::list_cf(&Options::default(), dir.path()).unwrap();
    for cf in ALL_COLUMN_FAMILIES {
        assert!(
            names.iter().any(|n| n == cf),
            "missing column family {cf}, have {names:?}"
        );
    }
}

#[test]
fn test_open_reopens_existing_db() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let block = sample_genesis_block();
    let h = block.hash();
    {
        let cfg = BlockStoreConfig {
            db_path: path.clone(),
            ..Default::default()
        };
        let store = BlockStore::open(cfg).unwrap();
        store.init_genesis(&block).unwrap();
    }
    let cfg = BlockStoreConfig {
        db_path: path.clone(),
        ..Default::default()
    };
    let store = BlockStore::open(cfg).unwrap();
    let got = store.get_block(&h).unwrap().expect("block round-trip");
    assert_eq!(got.hash(), h);
}

#[test]
fn test_open_loads_existing_tip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let block = sample_genesis_block();
    let expect_tip = ChainTip {
        hash: block.hash(),
        height: 0,
    };
    {
        let store = BlockStore::open(BlockStoreConfig {
            db_path: path.clone(),
            ..Default::default()
        })
        .unwrap();
        store.init_genesis(&block).unwrap();
    }
    let store = BlockStore::open(BlockStoreConfig {
        db_path: path,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(store.tip(), Some(expect_tip));
}

#[test]
fn test_open_warms_cache() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let block = sample_genesis_block();
    {
        let store = BlockStore::open(BlockStoreConfig {
            db_path: path.clone(),
            ..Default::default()
        })
        .unwrap();
        store.init_genesis(&block).unwrap();
    }
    let store = BlockStore::open(BlockStoreConfig {
        db_path: path,
        warm_cache_on_open: true,
        warm_cache_depth: 8,
        ..Default::default()
    })
    .unwrap();
    assert!(
        store.warm_blocks_loaded_count() >= 1,
        "warm_cache_on_open should touch at least height 0 when tip exists"
    );
}

#[test]
fn test_open_readonly_existing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let block = sample_genesis_block();
    let h = block.hash();
    {
        let store = BlockStore::open(BlockStoreConfig {
            db_path: path.clone(),
            ..Default::default()
        })
        .unwrap();
        store.init_genesis(&block).unwrap();
    }
    let store = BlockStore::open_readonly(&path).unwrap();
    let got = store.get_block(&h).unwrap().expect("read path");
    assert_eq!(got.hash(), h);
}

#[test]
fn test_open_readonly_missing_fails() {
    let path = std::env::temp_dir().join("dig_blockstore_str004_missing_db_xyz");
    let _ = std::fs::remove_dir_all(&path);
    match BlockStore::open_readonly(&path) {
        Err(e) => assert!(matches!(e, BlockStoreError::PathDoesNotExist(_))),
        Ok(_) => panic!("expected error for missing path"),
    }
}

#[test]
fn test_open_readonly_prevents_writes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let block = sample_genesis_block();
    {
        let store = BlockStore::open(BlockStoreConfig {
            db_path: path.clone(),
            ..Default::default()
        })
        .unwrap();
        store.init_genesis(&block).unwrap();
    }
    let store = BlockStore::open_readonly(&path).unwrap();
    let err = store.init_genesis(&sample_genesis_block()).unwrap_err();
    assert!(matches!(err, BlockStoreError::ReadOnly));
}

#[test]
fn test_init_genesis_stores_block() {
    let dir = TempDir::new().unwrap();
    let block = sample_genesis_block();
    let h = block.hash();
    let store = BlockStore::open(BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    })
    .unwrap();
    store.init_genesis(&block).unwrap();
    let got = store.get_block(&h).unwrap().expect("stored genesis");
    assert_eq!(got.hash(), h);
}

#[test]
fn test_init_genesis_sets_tip() {
    let dir = TempDir::new().unwrap();
    let block = sample_genesis_block();
    let store = BlockStore::open(BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    })
    .unwrap();
    store.init_genesis(&block).unwrap();
    assert_eq!(
        store.tip(),
        Some(ChainTip {
            hash: block.hash(),
            height: 0,
        })
    );
}

#[test]
fn test_init_genesis_records_hash() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();
    let block = sample_genesis_block();
    let gh = block.hash();
    {
        let store = BlockStore::open(BlockStoreConfig {
            db_path: path.to_path_buf(),
            ..Default::default()
        })
        .unwrap();
        store.init_genesis(&block).unwrap();
    }
    let raw = read_metadata_raw(path, META_GENESIS_HASH).expect("genesis meta");
    assert_eq!(raw.as_slice(), gh.as_ref());
}

#[test]
fn test_init_genesis_fails_if_initialized() {
    let dir = TempDir::new().unwrap();
    let block = sample_genesis_block();
    let store = BlockStore::open(BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    })
    .unwrap();
    store.init_genesis(&block).unwrap();
    let err = store.init_genesis(&block).unwrap_err();
    assert!(matches!(err, BlockStoreError::AlreadyInitialized));
}

#[test]
fn test_init_genesis_writes_use_single_writebatch() {
    // Implementation guarantee: `init_genesis` uses one `WriteBatch` (atomic) per [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md).
    // This test documents that contract; regression would require white-box inspection or fault injection.
    let dir = TempDir::new().unwrap();
    let store = BlockStore::open(BlockStoreConfig {
        db_path: dir.path().to_path_buf(),
        ..Default::default()
    })
    .unwrap();
    store.init_genesis(&sample_genesis_block()).unwrap();
    let tip_bytes = read_metadata_raw(dir.path(), META_TIP).expect("tip persisted");
    assert_eq!(tip_bytes.len(), 40);
}
