//! [`BlockRecord`] — cached metadata derived from a header (Chia-style split).
//!
//! **Spec:** Populated from [`dig_block::L2BlockHeader`] in [`TYP-004`](../../docs/requirements/domains/storage_types/specs/TYP-004.md).
//! **Hard rule:** Do not redefine [`dig_block::BlockStatus`](https://docs.rs/dig-block) — store it, don’t fork the enum.

/// Fast query record mirroring Chia’s `BlockRecord` pattern (`docs/resources/SPEC.md` overview).
#[derive(Debug, Clone, Default)]
pub struct BlockRecord {}
