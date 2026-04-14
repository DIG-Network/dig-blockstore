# dig-blockstore Requirements

This directory contains the formal requirements for the dig-blockstore crate,
following the same two-tier requirements structure as dig-gossip and dig-block
with full traceability.

## Quick Links

- [SCHEMA.md](SCHEMA.md) — Data model and conventions
- [REQUIREMENTS_REGISTRY.yaml](REQUIREMENTS_REGISTRY.yaml) — Central domain registry
- [domains/](domains/) — All requirement domains

## Structure

```
requirements/
├── README.md                    # This file
├── SCHEMA.md                    # Data model and conventions
├── REQUIREMENTS_REGISTRY.yaml   # Central registry
├── IMPLEMENTATION_ORDER.md      # Phased implementation checklist
└── domains/
    ├── crate_structure/         # STR-* Crate layout, dependencies, traits, test infra
    ├── storage_types/           # TYP-* BlockRecord, StoredCheckpoint, ChainTip, Config, Stats
    ├── key_encoding/            # KEY-* Hash keys, height keys, epoch keys, metadata keys
    ├── block_storage/           # BLK-* put/get blocks, headers, batch, prefetch, async, attestation
    ├── canonical_chain/         # CAN-* Dual-layer index, mmap, set_canonical, extend_chain, tip
    ├── rollback_reorg/          # ROR-* Rollback, find_common_ancestor, apply_reorg, fork preservation
    ├── checkpoint_storage/      # CKP-* put/get checkpoint, latest, range queries
    ├── pruning/                 # PRN-* Block pruning, checkpoint pruning, compaction filter
    ├── caching/                 # CAC-* Sharded LRU, block/header/record caches, warming
    ├── serialization/           # SER-* Zstd dict compression, bincode, wire-format, round-trips
    ├── snapshot/                # SNP-* Export, import, manifest, checksum
    └── error_types/             # ERR-* BlockStoreError enum
```

## Three-Document Pattern

Each domain contains:

| File | Purpose |
|------|---------|
| `NORMATIVE.md` | Authoritative requirement statements (MUST/SHOULD/MAY) |
| `VERIFICATION.md` | QA approach and status per requirement |
| `TRACKING.yaml` | Machine-readable status, tests, and notes |

## Specification Files

Individual requirement specifications are in each domain's `specs/` subdirectory.

## Reference Document

All requirements are derived from:
- [SPEC.md](../resources/SPEC.md) — dig-blockstore specification

## Requirement Count

| Domain | Prefix | Count |
|--------|--------|-------|
| Crate Structure | STR | 5 |
| Storage Types | TYP | 8 |
| Key Encoding | KEY | 4 |
| Block Storage | BLK | 10 |
| Canonical Chain | CAN | 7 |
| Rollback & Reorg | ROR | 5 |
| Checkpoint Storage | CKP | 4 |
| Pruning | PRN | 5 |
| Caching | CAC | 6 |
| Serialization | SER | 5 |
| Snapshot | SNP | 4 |
| Error Types | ERR | 3 |
| **Total** | | **66** |
