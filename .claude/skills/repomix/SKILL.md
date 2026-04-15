# Repomix — Context Packing Skill

## When to Use

Use Repomix **before implementing any requirement**. Pack the relevant scope so the LLM has full awareness of the code being modified.

## HARD RULE

**MUST pack context before writing implementation code.** Fresh context prevents redundant work and missed patterns.

## Commands

### Pack Implementation

```bash
npx repomix@latest src -o .repomix/pack-src.xml
```

### Pack Tests (CRITICAL for TDD)

```bash
npx repomix@latest tests -o .repomix/pack-tests.xml
```

### Pack Requirements by Domain

```bash
# block_storage
npx repomix@latest docs/requirements/domains/block_storage -o .repomix/pack-block-storage-reqs.xml

# caching
npx repomix@latest docs/requirements/domains/caching -o .repomix/pack-caching-reqs.xml

# canonical_chain
npx repomix@latest docs/requirements/domains/canonical_chain -o .repomix/pack-canonical-chain-reqs.xml

# checkpoint_storage
npx repomix@latest docs/requirements/domains/checkpoint_storage -o .repomix/pack-checkpoint-storage-reqs.xml

# crate_structure
npx repomix@latest docs/requirements/domains/crate_structure -o .repomix/pack-crate-structure-reqs.xml

# error_types
npx repomix@latest docs/requirements/domains/error_types -o .repomix/pack-error-types-reqs.xml

# key_encoding
npx repomix@latest docs/requirements/domains/key_encoding -o .repomix/pack-key-encoding-reqs.xml

# pruning
npx repomix@latest docs/requirements/domains/pruning -o .repomix/pack-pruning-reqs.xml

# rollback_reorg
npx repomix@latest docs/requirements/domains/rollback_reorg -o .repomix/pack-rollback-reorg-reqs.xml

# serialization
npx repomix@latest docs/requirements/domains/serialization -o .repomix/pack-serialization-reqs.xml

# snapshot
npx repomix@latest docs/requirements/domains/snapshot -o .repomix/pack-snapshot-reqs.xml

# storage_types
npx repomix@latest docs/requirements/domains/storage_types -o .repomix/pack-storage-types-reqs.xml

# All requirements at once
npx repomix@latest docs/requirements -o .repomix/pack-requirements.xml
```

### Pack the Full Spec

```bash
npx repomix@latest docs/resources -o .repomix/pack-spec.xml
```

### Pack with Compression

```bash
npx repomix@latest src --compress -o .repomix/pack-src-compressed.xml
```

### Pack Multiple Scopes

```bash
npx repomix@latest src tests -o .repomix/pack-impl-and-tests.xml
```

## Workflow Integration

| Step | Pack Command |
|------|-------------|
| Before writing tests | `npx repomix@latest tests -o .repomix/pack-tests.xml` |
| Before implementing | `npx repomix@latest src -o .repomix/pack-src.xml` |
| Cross-domain work | Pack both domains' requirements |

## Notes

- `.repomix/` is gitignored — pack files are never committed
- Regenerate packs when switching requirements
- Use `--compress` for large scopes to manage token count
- Pack requirements alongside code for spec compliance checks

## Full Documentation

See `docs/prompt/tools/repomix.md` for complete reference.
