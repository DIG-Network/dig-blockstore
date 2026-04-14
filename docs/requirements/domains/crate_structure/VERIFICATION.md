# Crate Structure - Verification Matrix

| Field | Value |
|-------|-------|
| **Domain** | Crate Structure |
| **Prefix** | STR |
| **Normative** | [NORMATIVE.md](NORMATIVE.md) |
| **Tracking** | [TRACKING.yaml](TRACKING.yaml) |

---

## Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| STR-001 | done | Cargo.toml Dependencies | `tests/crate_structure/str_001_cargo_deps.rs` parses `Cargo.toml` for all 16 dependencies, version pins, and required feature flags; spawns `cargo check` for end-to-end resolution. |
| STR-002 | done | Module Hierarchy | `tests/crate_structure/str_002_module_hierarchy.rs` asserts on-disk paths, imports required public items, validates `CF_*`/`META_*` strings, and runs `cargo check`. |
| STR-003 | done | Public Re-exports | `tests/crate_structure/str_003_reexports.rs` imports the full STR-003 surface from the crate root, asserts CF/META string values, and exercises encoding helpers (including epoch round-trip). |
| STR-004 | done | BlockStore Constructor | `tests/crate_structure/str_004_constructor.rs` covers open/reopen, CF listing, tip reload, cache warming, readonly semantics, genesis storage/tip/META_GENESIS_HASH, double-init, and WriteBatch documentation. |
| STR-005 | done | Test Infrastructure | `tests/crate_structure/str_005_test_infra.rs` exercises `tests/common/mod.rs`: temp dir create/cleanup, deterministic `test_block`/`test_header`, `test_config` small caches + blob/compression off, `build_chain` length/linking/heights/single-genesis. `BlockStoreConfig` carries TYP-008-oriented fields (wired in later reqs). |
