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
| CAN-005 | gap | extend_chain | Call extend_chain with a new block, verify block is stored, canonical, and tip is updated. Call again with same block, verify returns false (duplicate). Verify chain tip hash and height match the new block. |
| CAN-006 | gap | get_hash_by_height | Store canonical blocks, verify get_hash_by_height returns correct hash for each height. Disable mmap and verify fallback to CF_CANONICAL works. Verify get_block_by_height, get_header_by_height, get_record_by_height delegate correctly. Verify get_epoch_block_hashes returns correct range. |
| CAN-007 | done | Chain Tip Tracking | `tests/can_007_tests.rs` — `tip()` None on empty, `height()` accessor, `set_tip()` persistence to CF_METADATA/META_TIP + in-memory RwLock update, 40-byte encoding match (`hash[0..32] || height_LE[32..40]`), reopen durability, read-only guard, `init_genesis` sets tip, overwrite semantics. 8 tests. |
