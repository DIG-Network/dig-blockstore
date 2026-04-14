//! [`StoredCheckpoint`] — checkpoint + attestation metadata persisted per epoch.
//!
//! **Spec:** [`TYP-005`](../../docs/requirements/domains/storage_types/specs/TYP-005.md), checkpoint domain [`CKP-*`](../../docs/requirements/domains/checkpoint_storage/NORMATIVE.md).

/// Stored representation in `CF_CHECKPOINTS` ([`crate::constants::CF_CHECKPOINTS`]).
#[derive(Debug, Clone, Default)]
pub struct StoredCheckpoint {}
