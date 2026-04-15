# Canonical Chain - Verification Matrix

| Field | Value |
|-------|-------|
| **Domain** | Canonical Chain |
| **Prefix** | CAN |
| **Normative** | [NORMATIVE.md](NORMATIVE.md) |
| **Tracking** | [TRACKING.yaml](TRACKING.yaml) |

---

## Verification Matrix

| ID | Status | Summary | Verification Approach |
|----|--------|---------|----------------------|
| CAN-001 | done | Dual-Layer Canonical Index | `tests/can_001_tests.rs` — dual write proof, delete/corrupt reopen rebuild, `disable_canonical_bin_acceleration` RocksDB fallback, 200-height mmap vs CF byte equality. |
| CAN-002 | done | canonical.bin Memory-Mapped File | `tests/can_002_tests.rs` — dense size `(max_height+1)×32`, read/write roundtrip, growth, truncation, `memmap2`-backed `CanonicalDenseFile` in `src/canonical/mmap.rs`; wired into `BlockStore` via CAN-001 `CanonicalBin::Rw`. |
| CAN-003 | done | set_canonical | `tests/can_003_tests.rs` — success (CF + `canonical.bin`), `BlockNotInStore`, record flag via Orphaned→set_canonical, idempotent second call, same-height overwrite. |
| CAN-004 | done | set_canonical_batch | `tests/can_004_tests.rs` — `build_chain(10)` then one batch: every height in `CF_CANONICAL` + `canonical.bin`; empty `&[]` no-op; missing hash in the middle leaves prior CF rows unchanged (fail-fast before `WriteBatch`); `Orphaned`→batch flips `in_canonical_chain`; read-only rejects like BLK-009. |
| CAN-005 | done | extend_chain | `tests/can_005_tests.rs` — new block Ok(true) + stored + canonical + tip, duplicate Ok(false) + no state change, 10-block chain build, tip progression, hash retrieval, read-only guard. 6 tests. |
| CAN-006 | done | get_hash_by_height | `tests/can_006_tests.rs` — public `get_hash_by_height` (renamed from private `resolve_canonical_hash_at_height`), `get_header_by_height`, `get_epoch_block_hashes`. Mmap hot path + CF_CANONICAL fallback tested via `disable_canonical_bin_acceleration`. Epoch full/partial/empty tested via `dig_epoch::epoch_height_range`. 10 tests. |
| CAN-007 | done | Chain Tip Tracking | `tests/can_007_tests.rs` — `tip()` None on empty, `height()` accessor, `set_tip()` persistence to CF_METADATA/META_TIP + in-memory RwLock update, 40-byte encoding match (`hash[0..32] || height_LE[32..40]`), reopen durability, read-only guard, `init_genesis` sets tip, overwrite semantics. 8 tests. |
