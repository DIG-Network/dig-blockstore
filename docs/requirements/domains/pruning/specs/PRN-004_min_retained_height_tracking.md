# PRN-004: min_retained_height Tracking

| Link | Path |
|------|------|
| **Normative** | [NORMATIVE.md](../NORMATIVE.md) |
| **Verification** | [VERIFICATION.md](../VERIFICATION.md) |
| **Tracking** | [TRACKING.yaml](../TRACKING.yaml) |
| **Spec** | [SPEC.md](../../../../resources/SPEC.md) Section 10.4 |

---

## Summary

`min_retained_height` MUST be tracked in `CF_METADATA` under the `META_MIN_HEIGHT` key, updated by `prune_before_height()`, and loaded on startup to initialize the compaction filter threshold.

---

## Specification

### Persistence

- The `min_retained_height` value MUST be stored in `CF_METADATA` with the key `META_MIN_HEIGHT` (a UTF-8 string key).
- The value MUST be encoded as a big-endian u64 (8 bytes).

### Updates

- `prune_before_height(height)` MUST write `height` to `CF_METADATA` under `META_MIN_HEIGHT` after successful pruning.
- The write SHOULD be included in the same `WriteBatch` as the pruning deletions for atomicity.
- The in-memory `AtomicU64` MUST be updated to `height` using `Ordering::Release` after the `WriteBatch` succeeds.

### Startup Initialization

- On `BlockStore::open()`, the stored `min_retained_height` MUST be read from `CF_METADATA`.
- If the key exists, its value MUST be used to initialize the `AtomicU64`.
- If the key does not exist (fresh database), `min_retained_height` MUST default to 0.
- The initialized value MUST be passed to the compaction filter (if enabled).

### Invariants

- `min_retained_height` MUST be monotonically non-decreasing: it can only increase or stay the same.
- No block data below `min_retained_height` is guaranteed to exist in the store.

---

## Acceptance Criteria

- [ ] `min_retained_height` is persisted in `CF_METADATA` under key `META_MIN_HEIGHT`
- [ ] Value is encoded as big-endian u64
- [ ] `prune_before_height` updates the persisted value
- [ ] `prune_before_height` updates the `AtomicU64` with `Release` ordering
- [ ] On startup, value is loaded from `CF_METADATA` into `AtomicU64`
- [ ] Fresh database defaults to 0
- [ ] Value is monotonically non-decreasing
- [ ] Compaction filter receives the initialized value on startup

---

## Implementation Notes

- The `META_MIN_HEIGHT` key should be a well-known constant string, e.g., `b"min_retained_height"`.
- Including the metadata update in the pruning `WriteBatch` ensures that if the prune fails, `min_retained_height` is not incorrectly advanced.
- The `AtomicU64` update after the `WriteBatch` creates a brief window where the persisted value is ahead of the in-memory value. This is acceptable because the compaction filter uses the in-memory value, and a slightly stale threshold only means the filter is conservative (keeps entries it could drop).

---

## Test Plan

| Test Name | Type | Description |
|-----------|------|-------------|
| `test_min_retained_height_default` | integration | Open fresh database, verify min_retained_height is 0 |
| `test_min_retained_height_after_prune` | integration | Prune before height 100, verify META_MIN_HEIGHT is 100 in CF_METADATA |
| `test_min_retained_height_survives_restart` | integration | Prune, close store, reopen, verify min_retained_height restored |
| `test_min_retained_height_monotonic` | integration | Prune before 50, then prune before 30 (no-op), verify still 50 |
| `test_min_retained_height_atomic_u64_sync` | integration | Prune, verify AtomicU64 matches persisted value |
| `test_min_retained_height_compaction_filter_init` | integration | Set min_retained_height, reopen with compaction enabled, verify filter uses persisted value |

---

## Expected Test Files

- `tests/integration/pruning/test_prn_004_min_retained_height_tracking.rs`
