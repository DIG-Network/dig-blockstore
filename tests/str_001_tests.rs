//! # STR-001 — `Cargo.toml` dependency contract
//!
//! **Normative sources**
//! - [`docs/requirements/domains/crate_structure/specs/STR-001.md`](../docs/requirements/domains/crate_structure/specs/STR-001.md)
//! - [`docs/requirements/domains/crate_structure/NORMATIVE.md`](../docs/requirements/domains/crate_structure/NORMATIVE.md)
//! - [`docs/resources/SPEC.md`](../docs/resources/SPEC.md) §1.2 (crate dependency table)
//!
//! ## What this test file proves
//!
//! STR-001 requires sixteen named dependencies under `[dependencies]` with specific
//! minimum version pins and feature flags (`serde`/`tokio`). By parsing the workspace
//! manifest with `toml`, we **mechanically verify** the declaration matches the
//! requirement text — independent of whether any particular API is implemented yet.
//! That satisfies the acceptance criteria “all dependencies listed” and guards against
//! accidental removal during later refactors.
//!
//! A second test invokes `cargo check` as a subprocess. That proves the resolver can
//! fetch/build the graph end-to-end (“no missing-crate errors”), which is the other
//! STR-001 acceptance bullet.
//!
//! ## Design notes
//!
//! - We read `Cargo.toml` via `CARGO_MANIFEST_DIR` so the test stays relocatable.
//! - Path overrides for unpublished DIG crates (`dig-block`, stub `dig-epoch`) are
//!   allowed **only** when a `version = "0.1"` key is still present, preserving the
//!   semver intent spelled out in STR-001.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use toml::Value;

/// All `[dependencies]` names mandated by STR-001 / SPEC §1.2.
///
/// Order matches the spec table for readability when reading failures.
const REQUIRED_DEPS: &[&str] = &[
    "dig-block",
    "dig-epoch",
    "dig-constants",
    "chia-protocol",
    "chia-bls",
    "chia-sha2",
    "chia-traits",
    "rocksdb",
    "zstd",
    "bincode",
    "serde",
    "thiserror",
    "parking_lot",
    "lru",
    "tokio",
    "memmap2",
];

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

fn load_manifest() -> Value {
    let raw = fs::read_to_string(manifest_path()).expect("read Cargo.toml");
    raw.parse::<Value>().expect("parse Cargo.toml as TOML")
}

fn dep_table<'a>(manifest: &'a Value, key: &str) -> &'a toml::value::Table {
    manifest
        .get(key)
        .and_then(Value::as_table)
        .unwrap_or_else(|| panic!("missing [{key}] table in Cargo.toml"))
}

fn version_req(dep: &Value) -> Option<String> {
    match dep {
        Value::String(s) => Some(s.clone()),
        Value::Table(t) => t.get("version").and_then(Value::as_str).map(str::to_owned),
        _ => None,
    }
}

fn features_contain(dep: &Value, needle: &str) -> bool {
    let Value::Table(t) = dep else {
        return false;
    };
    let Some(Value::Array(feats)) = t.get("features") else {
        return false;
    };
    feats.iter().filter_map(Value::as_str).any(|f| f == needle)
}

/// **Requirement:** STR-001 — every direct dependency name exists and carries the
/// expected minimum version pin (or path+version for bootstrap crates).
#[test]
fn test_cargo_toml_has_all_deps() {
    let manifest = load_manifest();
    let deps = dep_table(&manifest, "dependencies");

    for name in REQUIRED_DEPS {
        let Some(entry) = deps.get(*name) else {
            panic!("missing dependency `{name}` — STR-001 requires all 16 crates");
        };

        let ver = version_req(entry).unwrap_or_else(|| {
            panic!(
                "dependency `{name}` must specify a version string or inline table with version = \"…\" (STR-001)"
            )
        });

        match *name {
            "dig-block" | "dig-epoch" | "dig-constants" => {
                assert!(
                    ver.starts_with("0.1"),
                    "`{name}` must use minimum 0.1.x per STR-001 / NORMATIVE; got {ver:?}"
                );
            }
            "chia-protocol" | "chia-bls" | "chia-sha2" | "chia-traits" => {
                assert!(
                    ver.starts_with("0.26"),
                    "`{name}` must use minimum 0.26.x per STR-001; got {ver:?}"
                );
            }
            _ => {}
        }
    }

    let serde = deps
        .get("serde")
        .expect("serde dependency missing (STR-001 / serde derive feature)");
    assert!(
        features_contain(serde, "derive"),
        "serde must enable the `derive` feature (STR-001)"
    );

    let tokio = deps
        .get("tokio")
        .expect("tokio dependency missing (STR-001 / full feature)");
    assert!(
        features_contain(tokio, "full"),
        "tokio must enable the `full` feature (STR-001)"
    );

    let rocks = deps
        .get("rocksdb")
        .expect("rocksdb dependency required (STR-001 / storage backend)");
    assert!(
        features_contain(rocks, "zstd"),
        "rocksdb must enable the `zstd` feature (STR-001 implementation notes / SPEC compression)"
    );
}

/// **Requirement:** STR-001 — `cargo check` exits successfully (graph resolves).
///
/// This is intentionally a subprocess call: it re-validates the manifest the same
/// way CI and developers do, catching issues that pure TOML inspection might miss
/// (feature unification, target-specific deps, etc.).
#[test]
fn test_cargo_check_succeeds() {
    let status = Command::new("cargo")
        .args(["check", "-q", "--manifest-path"])
        .arg(manifest_path())
        .status()
        .expect("spawn cargo check");
    assert!(
        status.success(),
        "cargo check must succeed for dig-blockstore (STR-001 acceptance)"
    );
}
