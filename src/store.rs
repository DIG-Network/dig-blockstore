//! `BlockStore` — RocksDB-backed persistent block and chain state.
//!
//! **Architecture**
//! - Owns column families described in [`crate::constants`] and canonical logic under [`crate::canonical`].
//! - Spec boundary: `docs/resources/SPEC.md` §16.1 (Crate boundary).
//!
//! **STR-002 scope:** Define the type; constructors land in [`STR-004`](../docs/requirements/domains/crate_structure/specs/STR-004.md).

/// Primary handle for all block persistence APIs ([`STR-002`](../docs/requirements/domains/crate_structure/specs/STR-002.md)).
#[derive(Debug, Default)]
pub struct BlockStore {}
