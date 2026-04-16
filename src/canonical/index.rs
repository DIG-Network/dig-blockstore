//! Canonical chain index coordination ([`CAN-001`](../../docs/requirements/domains/canonical_chain/specs/CAN-001.md)).
//!
//! Height→hash dual storage (`canonical.bin` + `CF_CANONICAL`) is managed by
//! [`crate::store::BlockStoreInner`]. Public API: [`crate::store::BlockStore::get_hash_by_height`],
//! [`crate::store::BlockStore::set_canonical`], [`crate::store::BlockStore::set_canonical_batch`].
