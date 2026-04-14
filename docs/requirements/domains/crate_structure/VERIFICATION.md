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
| STR-003 | gap | Public Re-exports | Write a test that imports every re-exported symbol from the crate root (`use dig_blockstore::*`). Compilation of the test confirms all re-exports are present. |
| STR-004 | gap | BlockStore Constructor | Test `open()` with a temp dir, verify DB files are created and all CFs exist. Test `open_readonly()` on an existing store. Test `init_genesis()` stores the genesis block and sets tip to height 0. |
| STR-005 | gap | Test Infrastructure | Verify temp dir helper creates and cleans up directories. Verify test block helper produces blocks with deterministic hashes. Verify chain builder produces N blocks with correct parent-child linking. |
