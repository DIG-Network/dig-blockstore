# Canonical Chain - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Canonical Chain |
| **Prefix** | CAN |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### CAN-001: Dual-Layer Canonical Index

The canonical chain **MUST** use two layers:

1. **mmap hot path** via `canonical.bin` memory-mapped file for O(1) lookups (~10ns per lookup via OS page cache).
2. **CF_CANONICAL cold path** in RocksDB as the durable, crash-recoverable backup (1-10us per lookup).

`canonical.bin` **MUST** be rebuilt from `CF_CANONICAL` on startup if the file is missing or corrupt. The mmap layer is an acceleration cache; `CF_CANONICAL` is the source of truth for recovery.

**Spec reference:** SPEC Section 6.1

---

### CAN-002: canonical.bin Memory-Mapped File

`canonical.bin` **MUST** be a dense array of 32-byte hashes. The hash at height `h` is located at byte offset `h * 32`. The file grows as the chain extends.

The file **MUST** use `memmap2` for zero-copy access via the OS page cache. Expected performance is ~10ns per pointer dereference lookup versus 1-10us for RocksDB.

```
Layout:  [hash_0: 32B][hash_1: 32B][hash_2: 32B]...
Offset:  height * 32
Size:    (max_height + 1) * 32
```

**Spec reference:** SPEC Section 6.1

---

### CAN-003: set_canonical

`set_canonical(&self, hash: &Bytes32) -> Result<()>` **MUST**:

1. Verify the block exists in the store; return `BlockNotInStore` error if not found.
2. Look up the height from the block header.
3. Write `height -> hash` to `CF_CANONICAL`.
4. Update the mmap file at `offset = height * 32`.
5. Update `BlockRecord.in_canonical_chain = true`.

```rust
pub fn set_canonical(&self, hash: &Bytes32) -> Result<()> {
    let record = self.get_record(hash)?
        .ok_or(BlockStoreError::BlockNotInStore(*hash))?;
    let height = record.height;

    // Write to CF_CANONICAL
    let cf = self.cf_canonical();
    self.db.put_cf(&cf, height.to_be_bytes(), hash.as_ref())?;

    // Update mmap
    self.mmap_index.write_hash(height, hash)?;

    // Update cached record
    self.update_record_canonical(hash, true)?;
    Ok(())
}
```

**Spec reference:** SPEC Section 6.2

---

### CAN-004: set_canonical_batch

`set_canonical_batch(&self, hashes: &[Bytes32]) -> Result<()>` **MUST** use a single RocksDB `WriteBatch` for all canonical updates. This ensures atomicity — either all hashes are marked canonical or none are.

This method is used during reorg to set the new canonical chain atomically.

```rust
pub fn set_canonical_batch(&self, hashes: &[Bytes32]) -> Result<()> {
    let mut batch = WriteBatch::default();
    let cf = self.cf_canonical();

    for hash in hashes {
        let record = self.get_record(hash)?
            .ok_or(BlockStoreError::BlockNotInStore(*hash))?;
        batch.put_cf(&cf, record.height.to_be_bytes(), hash.as_ref());
    }

    self.db.write(batch)?;

    // Update mmap and records after durable write
    for hash in hashes {
        let record = self.get_record(hash)?.unwrap();
        self.mmap_index.write_hash(record.height, hash)?;
        self.update_record_canonical(hash, true)?;
    }
    Ok(())
}
```

**Spec reference:** SPEC Section 6.2

---

### CAN-005: extend_chain

`extend_chain(&self, block: &L2Block) -> Result<bool>` **MUST**:

1. Store the block via `put` (block + header + record).
2. Set the block as canonical via `set_canonical`.
3. Update the chain tip atomically.
4. Return `false` if the block is a duplicate (already stored).

This is the primary block ingestion API for normal chain-following operation.

```rust
pub fn extend_chain(&self, block: &L2Block) -> Result<bool> {
    if self.has_block(&block.hash())? {
        return Ok(false); // duplicate
    }
    self.put(block)?;
    self.set_canonical(&block.hash())?;
    self.set_tip(ChainTip {
        hash: block.hash(),
        height: block.header.height,
    })?;
    Ok(true)
}
```

**Spec reference:** SPEC Section 6.3

---

### CAN-006: get_hash_by_height

`get_hash_by_height(&self, height: u64) -> Result<Option<Bytes32>>` **MUST**:

1. First check the mmap `canonical.bin` at offset `height * 32`.
2. If mmap is unavailable, fall back to `CF_CANONICAL` with big-endian height key.

Derived convenience methods **MUST** delegate through `get_hash_by_height`:

- `get_block_by_height(height) -> Result<Option<L2Block>>`
- `get_header_by_height(height) -> Result<Option<L2BlockHeader>>`
- `get_record_by_height(height) -> Result<Option<BlockRecord>>`
- `get_epoch_block_hashes(epoch) -> Result<Vec<Bytes32>>` (uses `dig-epoch` for height range)

```rust
pub fn get_hash_by_height(&self, height: u64) -> Result<Option<Bytes32>> {
    // Hot path: mmap
    if let Some(hash) = self.mmap_index.read_hash(height)? {
        return Ok(Some(hash));
    }
    // Cold path: RocksDB
    let cf = self.cf_canonical();
    match self.db.get_cf(&cf, height.to_be_bytes())? {
        Some(bytes) => Ok(Some(Bytes32::try_from(bytes.as_slice())?)),
        None => Ok(None),
    }
}
```

**Spec reference:** SPEC Section 6.1

---

### CAN-007: Chain Tip Tracking

`tip() -> Option<ChainTip>` **MUST** return the current chain tip (hash + height). `height() -> Option<u64>` is a convenience accessor that returns `tip().map(|t| t.height)`.

The chain tip **MUST** be updated atomically on `extend_chain` and `rollback_to_height`. The tip is persisted in `CF_METADATA` under the `META_TIP` key as a 40-byte value: `hash (32 bytes) || height_LE (8 bytes)`.

```rust
// META_TIP value layout: [hash: 32B][height: 8B LE]
pub fn set_tip(&self, tip: ChainTip) -> Result<()> {
    let mut value = [0u8; 40];
    value[..32].copy_from_slice(tip.hash.as_ref());
    value[32..].copy_from_slice(&tip.height.to_le_bytes());
    let cf = self.cf_metadata();
    self.db.put_cf(&cf, META_TIP, &value)?;
    *self.tip.write() = Some(tip);
    Ok(())
}
```

**Spec reference:** SPEC Section 7
