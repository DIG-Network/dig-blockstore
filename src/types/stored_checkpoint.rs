//! [`StoredCheckpoint`] — checkpoint + attestation bundle persisted under [`crate::constants::CF_CHECKPOINTS`].
//!
//! **Normative:** [`TYP-005`](../../docs/requirements/domains/storage_types/specs/TYP-005.md),
//! [`NORMATIVE § TYP-005`](../../docs/requirements/domains/storage_types/NORMATIVE.md#typ-005-storedcheckpoint-struct),
//! SPEC §3.3 (`docs/resources/SPEC.md`).
//!
//! ## Serialization
//!
//! Values are **`bincode`**-encoded for `put_cf` / `get_cf` ([`SER-001`](../../docs/requirements/domains/serialization/specs/SER-001.md)
//! family convention). Keys use [`crate::encoding::epoch_key`] (8-byte big-endian [`u64`]) per
//! [`KEY-003`](../../docs/requirements/domains/key_encoding/specs/KEY-003_epoch_keys.md) / [`TYP-005`](../../docs/requirements/domains/storage_types/specs/TYP-005.md).
//!
//! **`chia-bls` serde:** [`Signature`] and [`PublicKey`] require the **`serde`** feature on the
//! `chia-bls` dependency ([`STR-001`](../../docs/requirements/domains/crate_structure/specs/STR-001.md) —
//! manifest enables `features = ["serde"]` for bincode round-trips).
//!
//! ## Types from DIG / Chia
//!
//! - [`Checkpoint`], [`SignerBitmap`] — [`dig_block`](https://docs.rs/dig-block) (never redefined here; [`start.md`](../../docs/prompt/start.md) hard rule).
//! - [`Signature`], [`PublicKey`] — [`chia_bls`](https://docs.rs/chia-bls).

use chia_bls::{PublicKey, Signature};
use chia_protocol::Bytes32;
use dig_block::{Checkpoint, SignerBitmap};
use serde::{Deserialize, Serialize};

/// Checkpoint row for [`crate::constants::CF_CHECKPOINTS`]: core [`Checkpoint`] plus attestation metadata ([`TYP-005`](../../docs/requirements/domains/storage_types/specs/TYP-005.md)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCheckpoint {
    /// Core checkpoint payload from [`dig_block`] (CKP-001 field set: epoch, roots, counts, fees).
    pub checkpoint: Checkpoint,

    /// Which validators contributed to [`Self::aggregate_signature`] ([`SignerBitmap`] in dig-block).
    pub signer_bitmap: SignerBitmap,

    /// BLS aggregate G2 signature over the checkpoint commitment.
    pub aggregate_signature: Signature,

    /// BLS aggregate G1 public key corresponding to the signer set.
    pub aggregate_pubkey: PublicKey,

    /// Attestation score (signer count or weighted stake — interpretation is consensus-side).
    pub score: u64,

    /// Validator index that submitted this row (network-specific).
    pub submitter: u32,

    /// L1 height once the checkpoint is anchored (unset while pending).
    pub l1_height: Option<u32>,

    /// L1 coin id for the anchor transaction, if known.
    pub l1_coin_id: Option<Bytes32>,

    /// Local wall-clock time when this node persisted the value (Unix seconds).
    pub stored_at: u64,
}

impl StoredCheckpoint {
    /// Encode with the same `bincode` config the store will use for [`CF_CHECKPOINTS`](crate::constants::CF_CHECKPOINTS) values ([`TYP-005`](../../docs/requirements/domains/storage_types/specs/TYP-005.md)).
    ///
    /// **Rationale:** Centralizes option bytes so tests and future `put_checkpoint` agree on the wire shape.
    pub fn encode_bincode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Decode bytes previously produced by [`Self::encode_bincode`].
    pub fn decode_bincode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}
