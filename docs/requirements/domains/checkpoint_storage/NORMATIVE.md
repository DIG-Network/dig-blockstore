# Checkpoint Storage - Normative Requirements

- **Domain:** checkpoint_storage
- **Prefix:** CKP
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### CKP-001: Store Checkpoint (`put_checkpoint`)

`put_checkpoint(&self, checkpoint: &StoredCheckpoint) -> Result<()>`

1. MUST serialize `StoredCheckpoint` via bincode and write it to `CF_CHECKPOINTS` keyed by epoch (big-endian u64).
2. MUST be idempotent: overwrites any existing checkpoint for the same epoch without error.
3. The epoch key MUST be encoded as a big-endian 8-byte u64 to preserve sort order in RocksDB.

**Spec reference:** 9.1

---

### CKP-002: Get Checkpoint by Epoch (`get_checkpoint`)

`get_checkpoint(&self, epoch: u64) -> Result<Option<StoredCheckpoint>>`

1. MUST read from `CF_CHECKPOINTS` using the big-endian encoded epoch key.
2. MUST deserialize the stored bytes via bincode into a `StoredCheckpoint`.
3. MUST return `Ok(None)` if no checkpoint exists for the given epoch.

**Spec reference:** 9.2

---

### CKP-003: Get Latest Checkpoint (`get_latest_checkpoint`)

`get_latest_checkpoint(&self) -> Result<Option<StoredCheckpoint>>`

1. MUST use a RocksDB reverse iterator on `CF_CHECKPOINTS` to locate the entry with the highest epoch key.
2. MUST deserialize the found entry via bincode into a `StoredCheckpoint`.
3. MUST return `Ok(None)` if no checkpoints are stored.

**Spec reference:** 9.2

---

### CKP-004: Get Checkpoints in Range (`get_checkpoints_in_range`)

`get_checkpoints_in_range(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<StoredCheckpoint>>`

1. MUST use a RocksDB iterator seeking to the big-endian encoded `start_epoch` key.
2. MUST scan forward through all entries up to and including `end_epoch`.
3. MUST deserialize each entry via bincode and collect into a `Vec<StoredCheckpoint>`.
4. MUST return an empty `Vec` if no checkpoints exist in the specified range.

**Spec reference:** 9.2
