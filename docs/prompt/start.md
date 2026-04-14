# Start

## Immediate Actions

1. **Sync**
   ```bash
   git fetch origin && git pull origin main
   ```

2. **Check tools — ALL THREE MUST BE FRESH**
   ```bash
   npx gitnexus status          # GitNexus index fresh?
   npx gitnexus analyze         # Update if stale
   # SocratiCode: verify Docker running, index current
   codebase_status {}            # SocratiCode MCP status
   ```
   **Do not proceed until tools are confirmed operational.** Coding without tools leads to redundant work and missed dependencies.

3. **Pick work** — open `docs/requirements/IMPLEMENTATION_ORDER.md`
   - Choose the first `- [ ]` item
   - Every `- [x]` is done on main — skip it
   - Work phases in order: Phase 0 before Phase 1, etc.

4. **Pack context — BEFORE reading any code**
   ```bash
   npx repomix@latest src -o .repomix/pack-src.xml
   npx repomix@latest tests -o .repomix/pack-tests.xml
   ```

5. **Search with SocratiCode — BEFORE reading files**
   ```
   codebase_search { query: "blockstore rocksdb cache canonical chain rollback" }
   codebase_graph_query { filePath: "src/store.rs" }
   ```

6. **Read spec** — follow the full trace:
   - `NORMATIVE.md#PREFIX-NNN` → authoritative requirement
   - `specs/PREFIX-NNN.md` → detailed specification + **test plan**
   - `VERIFICATION.md` → how to verify
   - `TRACKING.yaml` → current status

7. **Continue** → [dt-wf-select.md](tree/dt-wf-select.md)

---

## Hard Requirements

1. **Block types come from dig-block** — never redefine L2BlockHeader, L2Block, AttestedBlock, Checkpoint, CheckpointSubmission, BlockStatus, SignerBitmap, ReceiptList.
2. **Epoch arithmetic comes from dig-epoch** — use epoch_for_block_height(), first_height_in_epoch(), epoch_checkpoint_height().
3. **Bytes32 from chia-protocol** — all hash keys use Bytes32 directly.
4. **SHA-256 from chia-sha2** — for hash verification on read-back.
5. **Streamable from chia-traits** — for wire-format interop.
6. **RocksDB for persistence** — column families, BlobDB, per-CF tuning.
7. **Zstd for block compression** — dictionary-trained, with plain fallback.
8. **Bincode for serialization** — matches dig-block's format.
9. **Idempotent writes** — inserting existing block returns Ok(false), not error.
10. **Canonical chain is an index** — height→hash mapping, blocks stored once by hash.
11. **Forks are kept** — non-canonical blocks remain accessible by hash.
12. **Dual-layer canonical** — mmap hot path + CF_CANONICAL cold path.
13. **TEST FIRST (TDD)** — write the failing test before writing implementation code.
14. **One requirement per commit** — don't batch unrelated work.
15. **Update tracking after each requirement** — VERIFICATION.md, TRACKING.yaml, IMPLEMENTATION_ORDER.md.

---

## Tech Stack

| Component | Crate | Version |
|-----------|-------|---------|
| Block types | `dig-block` | 0.1 |
| Epoch arithmetic | `dig-epoch` | 0.1 |
| Network constants | `dig-constants` | 0.1 |
| Hash type | `chia-protocol` | 0.26 |
| BLS types | `chia-bls` | 0.26 |
| SHA-256 | `chia-sha2` | 0.26 |
| Wire format | `chia-traits` | 0.26 |
| Storage backend | `rocksdb` | latest |
| Compression | `zstd` | latest |
| Serialization | `bincode` | latest |
| Serde framework | `serde` | 1 |
| Error derivation | `thiserror` | latest |
| Async runtime | `tokio` | 1.x |
| Concurrency | `parking_lot` | latest |
| LRU cache | `lru` | latest |
| Memory mapping | `memmap2` | latest |
| Testing | `tempfile` | 3 |
