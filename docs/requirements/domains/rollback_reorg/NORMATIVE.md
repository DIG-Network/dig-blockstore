# Rollback & Reorg - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Rollback & Reorg |
| **Prefix** | ROR |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### ROR-001: Rollback to Height

`rollback_to_height(&self, target_height: u64) -> Result<Vec<Bytes32>>` **MUST**:

1. Remove canonical mappings for all heights `> target_height` from `CF_CANONICAL`.
2. Truncate the mmap `canonical.bin` file to `(target_height + 1) * 32` bytes.
3. Update the chain tip to the block at `target_height`.
4. Mark rolled-back blocks as non-canonical in BlockRecord cache (`in_canonical_chain = false`).
5. Return the hashes of all reverted blocks (in descending height order).

Rollback **MUST NOT** delete the actual block data from `CF_BLOCKS`. Blocks are preserved for potential future reorg.

```rust
pub fn rollback_to_height(&self, target_height: u64) -> Result<Vec<Bytes32>> {
    // Validate (see ROR-005)
    let current_tip = self.tip().ok_or(BlockStoreError::NoTip)?;
    if target_height > current_tip.height {
        return Err(BlockStoreError::RollbackAboveTip {
            target: target_height, tip: current_tip.height,
        });
    }
    if target_height < self.min_retained_height()? {
        return Err(BlockStoreError::RollbackBelowMin {
            target: target_height, min: self.min_retained_height()?,
        });
    }

    let mut reverted = Vec::new();
    let mut batch = WriteBatch::default();
    let cf = self.cf_canonical();

    for h in (target_height + 1..=current_tip.height).rev() {
        if let Some(hash) = self.get_hash_by_height(h)? {
            batch.delete_cf(&cf, h.to_be_bytes());
            self.update_record_canonical(&hash, false)?;
            reverted.push(hash);
        }
    }

    self.db.write(batch)?;
    self.mmap_index.truncate(target_height)?;

    // Update tip to block at target_height
    let new_tip_hash = self.get_hash_by_height(target_height)?
        .ok_or(BlockStoreError::BlockNotInStore(Bytes32::default()))?;
    self.set_tip(ChainTip { hash: new_tip_hash, height: target_height })?;

    Ok(reverted)
}
```

**Spec reference:** SPEC Section 8.1

---

### ROR-002: Find Common Ancestor

`find_common_ancestor(&self, hash: &Bytes32, max_depth: u64) -> Result<Option<(Bytes32, u64)>>` **MUST**:

1. Starting from `hash`, walk the `parent_hash` chain.
2. At each step, check if the block is in the canonical chain: `get_hash_by_height(record.height) == Some(record.hash)`.
3. Return the first match as `(hash, height)`.
4. Stop after `max_depth` steps.
5. Return `None` if no common ancestor is found within `max_depth`.

```rust
pub fn find_common_ancestor(
    &self,
    hash: &Bytes32,
    max_depth: u64,
) -> Result<Option<(Bytes32, u64)>> {
    let mut current_hash = *hash;

    for _ in 0..max_depth {
        let record = match self.get_record(&current_hash)? {
            Some(r) => r,
            None => return Ok(None),
        };

        // Check if this block is canonical at its height
        if let Some(canonical_hash) = self.get_hash_by_height(record.height)? {
            if canonical_hash == current_hash {
                return Ok(Some((current_hash, record.height)));
            }
        }

        current_hash = record.parent_hash;
    }

    Ok(None)
}
```

**Spec reference:** SPEC Section 8.2

---

### ROR-003: Apply Reorg (Atomic)

`apply_reorg(&self, ancestor_height: u64, new_chain_hashes: &[Bytes32]) -> Result<ReorgResult>` **MUST**:

1. In a single `WriteBatch`:
   - Rollback canonical mappings to `ancestor_height` (delete entries for heights `> ancestor_height`).
   - Write new canonical mappings for `new_chain_hashes` via `set_canonical_batch` logic.
   - Update the chain tip to the last hash in `new_chain_hashes`.
2. Atomicity ensures no intermediate state is visible to concurrent readers.
3. Return a `ReorgResult` containing `reverted: Vec<Bytes32>` (hashes removed from canonical chain), `applied: Vec<Bytes32>` (hashes added to canonical chain), and `new_tip: ChainTip` (the new chain tip after reorg).

```rust
pub fn apply_reorg(
    &self,
    ancestor_height: u64,
    new_chain_hashes: &[Bytes32],
) -> Result<ReorgResult> {
    let current_tip = self.tip().ok_or(BlockStoreError::NoTip)?;
    let mut batch = WriteBatch::default();
    let cf = self.cf_canonical();
    let mut reverted = Vec::new();

    // (1) Remove old canonical entries above ancestor
    for h in (ancestor_height + 1..=current_tip.height).rev() {
        if let Some(hash) = self.get_hash_by_height(h)? {
            batch.delete_cf(&cf, h.to_be_bytes());
            self.update_record_canonical(&hash, false)?;
            reverted.push(hash);
        }
    }

    // (2) Set new canonical chain
    for hash in new_chain_hashes {
        let record = self.get_record(hash)?
            .ok_or(BlockStoreError::BlockNotInStore(*hash))?;
        batch.put_cf(&cf, record.height.to_be_bytes(), hash.as_ref());
    }

    // (3) Update tip in same batch
    let new_tip_hash = *new_chain_hashes.last()
        .ok_or(BlockStoreError::EmptyReorgChain)?;
    let new_tip_record = self.get_record(&new_tip_hash)?.unwrap();
    let mut tip_value = [0u8; 40];
    tip_value[..32].copy_from_slice(new_tip_hash.as_ref());
    tip_value[32..].copy_from_slice(&new_tip_record.height.to_le_bytes());
    batch.put_cf(&self.cf_metadata(), META_TIP, &tip_value);

    // Atomic write
    self.db.write(batch)?;

    // Post-commit: update mmap and records
    self.mmap_index.truncate(ancestor_height)?;
    for hash in new_chain_hashes {
        let record = self.get_record(hash)?.unwrap();
        self.mmap_index.write_hash(record.height, hash)?;
        self.update_record_canonical(hash, true)?;
    }

    let new_tip = ChainTip {
        hash: new_tip_hash,
        height: new_tip_record.height,
    };
    *self.tip.write() = Some(new_tip);

    Ok(ReorgResult {
        reverted,
        applied: new_chain_hashes.to_vec(),
        new_tip,
    })
}
```

**Spec reference:** SPEC Section 8.2

---

### ROR-004: Fork Preservation

Non-canonical blocks **MUST** remain stored in `CF_BLOCKS` and `CF_HEADERS`, retrievable by hash via `get_block` / `get_header`. Only the canonical index (`CF_CANONICAL`) changes during rollback and reorg.

Block data **MUST NOT** be deleted during rollback or reorg operations. This enables efficient reorg without requiring re-download of previously seen blocks from the network.

**Spec reference:** SPEC Section 1.1 (Design Principles)

---

### ROR-005: Rollback Boundary Validation

`rollback_to_height` **MUST** validate boundaries before performing any mutations:

- Return `RollbackBelowMin` if `target_height < min_retained_height` (after pruning has removed lower blocks).
- Return `RollbackAboveTip` if `target_height > current_tip.height`.
- Return `NoTip` if no chain tip is currently set.

```rust
// Error variants
pub enum BlockStoreError {
    RollbackBelowMin { target: u64, min: u64 },
    RollbackAboveTip { target: u64, tip: u64 },
    NoTip,
    // ...
}
```

**Spec reference:** SPEC Section 8.1

---

### ROR-006: Blocks to Revert

`blocks_to_revert(&self, target_height: u64) -> Result<Vec<Bytes32>>` **MUST**:

1. Return the canonical block hashes that would be reverted if the chain rolled back to `target_height`.
2. Return hashes in descending height order (tip first).
3. NOT perform any mutations (read-only query).
4. Return an empty vec if `target_height >= current_tip.height` or if no tip is set.

This method is used by the consensus layer to evaluate the cost of a potential rollback before committing to it.

```rust
pub fn blocks_to_revert(&self, target_height: u64) -> Result<Vec<Bytes32>> {
    let current_tip = match self.tip() {
        Some(t) => t,
        None => return Ok(Vec::new()),
    };
    if target_height >= current_tip.height {
        return Ok(Vec::new());
    }

    let mut reverted = Vec::new();
    for h in (target_height + 1..=current_tip.height).rev() {
        if let Some(hash) = self.get_hash_by_height(h)? {
            reverted.push(hash);
        }
    }
    Ok(reverted)
}
```

**Spec reference:** SPEC Section 8.2, 15.4
