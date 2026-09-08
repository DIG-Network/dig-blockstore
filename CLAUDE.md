# dig-blockstore — Project Context

## What This Is

dig-blockstore — Persistent block and checkpoint storage, chain canonicality, and pruning.

## Key Documents

| Document | Path | Purpose |
|----------|------|---------|
| Master Spec | `docs/resources/SPEC.md` | Authoritative crate specification |
| Requirements | `docs/requirements/README.md` | Traceable requirements by domain |
| Implementation Order | `docs/requirements/IMPLEMENTATION_ORDER.md` | Phased checklist |

---

## Tool Usage — MANDATORY ON EVERY PROMPT

### GitNexus — Impact analysis before editing

**ALWAYS run impact analysis before modifying any public symbol.**

```bash
npx gitnexus status          # Check index freshness
npx gitnexus analyze         # Update if stale
```

```
gitnexus_impact({target: "BlockStore", direction: "upstream"})
gitnexus_detect_changes({scope: "staged"})
```

**After every commit:** `npx gitnexus analyze` to keep the index current.

### Repomix — Pack context before implementing

**ALWAYS pack relevant scope before starting implementation work.**

```bash
npx repomix@latest src -o .repomix/pack-src.xml
npx repomix@latest tests -o .repomix/pack-tests.xml
npx repomix@latest docs/requirements -o .repomix/pack-requirements.xml
```

---

## Workflow Cycle

| Step | Action | Tool |
|------|--------|------|
| 0 | Sync repo, check tool freshness | `git pull`, `npx gitnexus status` |
| 1 | Pick next `- [ ]` from `IMPLEMENTATION_ORDER.md` | — |
| 2 | Pack context | Repomix |
| 3 | Read requirement spec | `docs/requirements/domains/{domain}/specs/{ID}.md` |
| 4 | Implement / test | TDD where applicable |
| 5 | Run tests, clippy, fmt | `cargo test`, `cargo clippy`, `cargo fmt` |
| 6 | Check impact | `gitnexus_detect_changes` |
| 7 | Update tracking | TRACKING.yaml, VERIFICATION.md, IMPLEMENTATION_ORDER.md |
| 8 | Commit + update index | `git commit`, `npx gitnexus analyze` |
