# Requirements Schema

This document defines the data model and conventions for all requirements in the
dig-blockstore project.

---

## Three-Document Pattern

Each domain has exactly three files in `docs/requirements/domains/{domain}/`:

| File | Purpose |
|------|---------|
| `NORMATIVE.md` | Authoritative requirement statements with MUST/SHOULD/MAY keywords |
| `VERIFICATION.md` | QA approach and verification status per requirement |
| `TRACKING.yaml` | Machine-readable status, test references, and implementation notes |

Each requirement also has a dedicated specification file in
`docs/requirements/domains/{domain}/specs/{PREFIX-NNN}.md`.

---

## Requirement ID Format

**Pattern:** `{PREFIX}-{NNN}`

- **PREFIX**: 2-4 letter domain identifier (uppercase)
- **NNN**: Zero-padded numeric ID starting at 001

| Domain | Directory | Prefix | Description |
|--------|-----------|--------|-------------|
| Crate Structure | `crate_structure/` | `STR` | Cargo.toml, module hierarchy, re-exports, test infrastructure |
| Storage Types | `storage_types/` | `TYP` | BlockRecord, StoredCheckpoint, ChainTip, BlockStoreConfig, StorageStats, constants |
| Key Encoding | `key_encoding/` | `KEY` | Hash keys, height keys, epoch keys, metadata keys |
| Block Storage | `block_storage/` | `BLK` | put/get blocks, headers, attestation, batch, prefetch, async API |
| Canonical Chain | `canonical_chain/` | `CAN` | Dual-layer index, mmap, set_canonical, extend_chain, chain tip |
| Rollback & Reorg | `rollback_reorg/` | `ROR` | Rollback, find_common_ancestor, apply_reorg, fork preservation |
| Checkpoint Storage | `checkpoint_storage/` | `CKP` | put/get checkpoint, latest, range queries |
| Pruning | `pruning/` | `PRN` | Block pruning, checkpoint pruning, compaction filter |
| Caching | `caching/` | `CAC` | Sharded LRU, block/header/record caches, cache warming |
| Serialization | `serialization/` | `SER` | Zstd dictionary compression, bincode, wire-format, round-trips |
| Snapshot | `snapshot/` | `SNP` | Export, import, manifest, checksum |
| Error Types | `error_types/` | `ERR` | BlockStoreError enum and error handling |

**Immutability:** Requirement IDs are permanent. Deprecate requirements rather
than renumbering.

---

## Requirement Keywords

Per RFC 2119:

| Keyword | Meaning | Impact |
|---------|---------|--------|
| **MUST** | Absolute requirement | Blocks "done" status if not met |
| **MUST NOT** | Absolute prohibition | Blocks "done" status if violated |
| **SHOULD** | Expected behavior; may be deferred with rationale | Phase 2+ polish items |
| **SHOULD NOT** | Discouraged behavior | Phase 2+ polish items |
| **MAY** | Optional, nice-to-have | Stretch goals |

---

## Status Values

| Status | Description |
|--------|-------------|
| `gap` | Not implemented |
| `partial` | Implementation in progress or incomplete |
| `implemented` | Code complete, awaiting verification |
| `verified` | Implemented and verified per VERIFICATION.md |
| `deferred` | Explicitly postponed with rationale |

---

## TRACKING.yaml Item Schema

```yaml
- id: PREFIX-NNN           # Requirement ID (required)
  section: "Section Name"  # Logical grouping within domain (required)
  summary: "Brief title"   # Human-readable description (required)
  status: gap              # One of: gap, partial, implemented, verified, deferred
  spec_ref: "docs/requirements/domains/{domain}/specs/{PREFIX-NNN}.md"
  tests: []                # Array of test names or ["manual"]
  notes: ""                # Implementation notes, blockers, or evidence
```

---

## Testing Requirements

All dig-blockstore requirements MUST be tested using:

### 1. Unit Tests (MUST)

All storage, caching, and encoding paths MUST be tested with:

1. **Create** a BlockStore with temp directory and test config
2. **Store** test blocks, headers, checkpoints
3. **Retrieve** and verify correctness
4. **Verify** error conditions, edge cases, and boundary values

### 2. Integration Tests (MUST for multi-domain requirements)

Tests MUST demonstrate correct interaction between domains by:
- Full block lifecycle (store → canonical → rollback → reorg)
- Write pipeline throughput under concurrent access
- Snapshot export/import round-trip
- Pruning with cache eviction

### 3. Performance Tests (SHOULD for storage requirements)

Performance-related requirements SHOULD include benchmarks:
- Sequential write throughput (blocks/sec)
- Random read latency (cache hit vs miss)
- Write pipeline vs single-block improvement
- Mmap vs RocksDB canonical lookup

### 4. Required Test Infrastructure

```toml
# Cargo.toml [dev-dependencies]
tempfile = "3"
rand = "0.8"
tokio = { version = "1", features = ["test-util", "macros"] }
```

```rust
use dig_blockstore::{BlockStore, BlockStoreConfig, BlockRecord, StoredCheckpoint};
use dig_blockstore::{ChainTip, StorageStats, BlockStoreError};
use dig_block::{L2BlockHeader, L2Block, AttestedBlock, Checkpoint, BlockStatus};
use chia_protocol::Bytes32;
```

---

## Master Spec Reference

All requirements trace back to the SPEC:
[SPEC.md](../resources/SPEC.md)
