//! # TYP-003 — Per–column-family RocksDB configuration
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`TYP-003.md`](../docs/requirements/domains/storage_types/specs/TYP-003.md)
//! - NORMATIVE: [`NORMATIVE.md`](../docs/requirements/domains/storage_types/NORMATIVE.md) §TYP-003
//! - Verification row: [`VERIFICATION.md`](../docs/requirements/domains/storage_types/VERIFICATION.md)
//!
//! ## Proof strategy
//!
//! Rust [`rocksdb::Options`](https://docs.rs/rocksdb) does not expose stable read-back getters for every
//! field we set (compaction style, BlobDB flags, bloom policy), so these tests **materialize a real DB**
//! with [`dig_blockstore::BlockStore::open`], drop the handle to release file locks, then parse the
//! on-disk `OPTIONS-*` snapshots RocksDB writes under the DB directory (see
//! [RocksDB options file](https://github.com/facebook/rocksdb/wiki/RocksDB-Options-File)).
//!
//! Each test maps to an acceptance row or test-plan entry in [`TYP-003.md`](../docs/requirements/domains/storage_types/specs/TYP-003.md):
//! we assert **observable** `CFOptions` fields (`compaction_style`, `filter_policy`, `compression`,
//! `enable_blob_files`, `min_blob_size`, `target_file_size_base`) for the relevant column family name.
//!
//! **Important:** Read-only reopen tests rely on STR-005 `test_config` using the same BlobDB flag as
//! [`BlockStoreConfig::default`] so CF descriptors stay compatible ([`store.rs`](../src/store.rs)
//! `open_readonly` path).

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
#[allow(dead_code)]
mod common;

use std::collections::HashMap;
use std::fs::{self, read_dir};
use std::path::{Path, PathBuf};

use dig_blockstore::{
    cf_options, BlockStore, BlockStoreConfig, CF_ATTESTED, CF_BLOCKS, CF_CANONICAL, CF_CHECKPOINTS,
    CF_HEADERS, CF_METADATA,
};

/// Concatenate every `OPTIONS-*` file RocksDB emitted so tests see all `[CFOptions "..."]` stanzas.
fn merged_options_dump(db_dir: &Path) -> String {
    let mut paths: Vec<PathBuf> = read_dir(db_dir)
        .expect("read_dir db")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("OPTIONS-"))
        })
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "expected at least one OPTIONS-* file under {}",
        db_dir.display()
    );
    let mut out = String::new();
    for p in paths {
        out.push_str(
            &fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display())),
        );
        out.push('\n');
    }
    out
}

/// Extract the `rocksdb` text blob between `[CFOptions "cf_name"]` and the next `[` section header.
fn section_after_header(content: &str, header: &str) -> String {
    let start = content
        .find(header)
        .unwrap_or_else(|| panic!("missing {header} in options dump (len {})", content.len()));
    let after = &content[start + header.len()..];
    let end = after
        .find("\n[")
        .or_else(|| after.find("\r\n["))
        .unwrap_or(after.len());
    after[..end].trim().to_string()
}

fn cf_options_section(content: &str, cf_name: &str) -> String {
    let header = format!("[CFOptions \"{cf_name}\"]");
    section_after_header(content, &header)
}

/// Bloom lives under **table** options in modern RocksDB option dumps, not always as a top-level
/// `filter_policy` key on `[CFOptions]` ([RocksDB options file example](https://github.com/facebook/rocksdb/blob/main/examples/rocksdb_option_file_example.ini)).
fn table_options_block_based_section(content: &str, cf_name: &str) -> String {
    let header = format!("[TableOptions/BlockBasedTable \"{cf_name}\"]");
    section_after_header(content, &header)
}

/// Parse `key=value` tokens RocksDB emits (skip stray continuations without `=`).
fn parse_kv_blob(section: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        m.insert(k.trim().to_string(), v.trim().to_string());
    }
    m
}

fn open_temp_store(enable_blob_db: bool) -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = common::temp_blockstore_dir();
    let mut cfg = common::test_config(path.clone());
    cfg.enable_blob_db = enable_blob_db;

    let _store = BlockStore::open(cfg).expect("BlockStore::open");

    drop(_store);

    (dir, path)
}

fn kv_for_cf(db_dir: &Path, cf: &str) -> HashMap<String, String> {
    let dump = merged_options_dump(db_dir);
    let section = cf_options_section(&dump, cf);
    parse_kv_blob(&section)
}

fn table_kv_for_cf(db_dir: &Path, cf: &str) -> HashMap<String, String> {
    let dump = merged_options_dump(db_dir);
    let section = table_options_block_based_section(&dump, cf);
    parse_kv_blob(&section)
}

#[test]
fn test_blocks_cf_universal_compaction() {
    // **Acceptance:** CF_BLOCKS uses Universal compaction ([`TYP-003.md`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).

    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_BLOCKS);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleUniversal"),
        "CF_BLOCKS compaction_style: {:?}",
        kv.get("compaction_style")
    );
}

#[test]
fn test_blocks_cf_blobdb_enabled_when_config_true() {
    // **Acceptance / test plan:** BlobDB active for CF_BLOCKS when `config.enable_blob_db` ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).

    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_BLOCKS);

    let blob_on = kv
        .get("enable_blob_files")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    assert!(blob_on, "enable_blob_files missing/false: {kv:?}");
    assert_eq!(
        kv.get("min_blob_size").map(String::as_str),
        Some("512"),
        "min_blob_size"
    );
    assert_eq!(
        kv.get("blob_compression_type").map(String::as_str),
        Some("kZSTD"),
        "blob_compression_type"
    );
}

#[test]
fn test_blocks_cf_blobdb_off_when_config_false() {
    let (_dir, path) = open_temp_store(false);
    let kv = kv_for_cf(&path, CF_BLOCKS);

    let blob_on = kv
        .get("enable_blob_files")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    assert!(
        !blob_on,
        "BlobDB should be off when enable_blob_db=false: {kv:?}"
    );
}

#[test]
fn test_blocks_cf_does_not_set_bloom_filter_policy() {
    // **Acceptance:** CF_BLOCKS does not install a table filter / bloom ([`TYP-003`](../docs/requirements/domains/storage_types/specs/TYP-003.md)).

    let (_dir, path) = open_temp_store(true);
    let tkv = table_kv_for_cf(&path, CF_BLOCKS);
    assert!(
        matches!(
            tkv.get("filter_policy").map(String::as_str),
            None | Some("nullptr")
        ),
        "expected no bloom filter policy on blocks table, got {:?}",
        tkv.get("filter_policy")
    );
}

#[test]
fn test_headers_cf_level_compaction() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_HEADERS);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleLevel")
    );
}

#[test]
fn test_headers_cf_has_bloom_filter() {
    let (_dir, path) = open_temp_store(true);
    let tkv = table_kv_for_cf(&path, CF_HEADERS);

    let fp = tkv
        .get("filter_policy")
        .map(String::as_str)
        .unwrap_or("missing");
    assert!(
        fp.contains("Bloom") || fp.contains("bloom"),
        "expected BuiltinBloom-style filter_policy in BlockBasedTable options, got {fp:?}"
    );
}

#[test]
fn test_headers_cf_no_compression() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_HEADERS);
    assert_eq!(
        kv.get("compression").map(String::as_str),
        Some("kNoCompression")
    );
}

#[test]
fn test_attested_cf_level_and_bloom() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_ATTESTED);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleLevel")
    );
    let tkv = table_kv_for_cf(&path, CF_ATTESTED);
    let fp = tkv
        .get("filter_policy")
        .map(String::as_str)
        .unwrap_or("missing");
    assert!(
        fp.contains("Bloom") || fp.contains("bloom"),
        "attested bloom: {fp:?}"
    );
}

#[test]
fn test_canonical_cf_no_bloom_no_compression() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_CANONICAL);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleLevel")
    );
    let tkv = table_kv_for_cf(&path, CF_CANONICAL);
    assert!(matches!(
        tkv.get("filter_policy").map(String::as_str),
        None | Some("nullptr")
    ));
    assert_eq!(
        kv.get("compression").map(String::as_str),
        Some("kNoCompression")
    );
}

#[test]
fn test_checkpoints_cf_large_target_file_size_base() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_CHECKPOINTS);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleLevel")
    );

    let want = format!("{}", 256u64 * 1024 * 1024);
    assert_eq!(
        kv.get("target_file_size_base").map(String::as_str),
        Some(want.as_str())
    );
}

#[test]
fn test_metadata_cf_level_compaction() {
    let (_dir, path) = open_temp_store(true);
    let kv = kv_for_cf(&path, CF_METADATA);
    assert_eq!(
        kv.get("compaction_style").map(String::as_str),
        Some("kCompactionStyleLevel")
    );
}

#[test]
fn test_column_family_descriptors_public_api_order_matches_typ001() {
    let cfg = BlockStoreConfig::default();
    let descs = cf_options::column_family_descriptors(&cfg);

    let names: Vec<&str> = descs.iter().map(|d| d.name()).collect();

    assert_eq!(
        names,
        vec![
            CF_BLOCKS,
            CF_HEADERS,
            CF_ATTESTED,
            CF_CANONICAL,
            CF_CHECKPOINTS,
            CF_METADATA,
        ],
        "descriptor order must track ALL_COLUMN_FAMILIES for stable open() wiring"
    );
}
