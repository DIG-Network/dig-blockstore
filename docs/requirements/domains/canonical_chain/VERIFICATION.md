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
| CAN-002 | gap | canonical.bin Memory-Mapped File | Store blocks at heights 0..N, verify canonical.bin file size equals `(N+1) * 32`. Read raw bytes at `height * 32` and confirm they match the expected block hash. Verify zero-copy access via memmap2. |
| CAN-003 | gap | set_canonical | Call set_canonical with a stored block hash, verify CF_CANONICAL contains the height-to-hash mapping, mmap file is updated, and BlockRecord.in_canonical_chain is true. Verify BlockNotInStore error for unknown hash. |
| CAN-004 | gap | set_canonical_batch | Call set_canonical_batch with multiple hashes, verify all are written atomically. Simulate partial failure and confirm no partial writes to CF_CANONICAL. Verify mmap and records are updated for all hashes. |
| CAN-005 | gap | extend_chain | Call extend_chain with a new block, verify block is stored, canonical, and tip is updated. Call again with same block, verify returns false (duplicate). Verify chain tip hash and height match the new block. |
| CAN-006 | gap | get_hash_by_height | Store canonical blocks, verify get_hash_by_height returns correct hash for each height. Disable mmap and verify fallback to CF_CANONICAL works. Verify get_block_by_height, get_header_by_height, get_record_by_height delegate correctly. Verify get_epoch_block_hashes returns correct range. |
| CAN-007 | gap | Chain Tip Tracking | Verify tip() returns None on empty store. After extend_chain, verify tip matches last block. After rollback, verify tip is updated. Verify META_TIP persistence by reopening the store and checking tip(). Verify 40-byte encoding: hash(32) + height_LE(8). |
