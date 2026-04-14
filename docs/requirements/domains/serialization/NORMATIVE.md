# Serialization - Normative Requirements

- **Domain:** serialization
- **Prefix:** SER
- **Crate:** dig-blockstore
- **Spec version:** 0.1.0

## Requirements

### SER-001: Block Serialization with Dictionary Compression

Full blocks MUST be serialized using the following write path: `L2Block` -> `bincode::serialize()` -> `zstd::compress_with_dictionary(dict, level=3)` -> `CF_BLOCKS`. The read path MUST reverse this: `CF_BLOCKS` -> `zstd::decompress_with_dictionary(dict)` -> `bincode::deserialize()` -> `L2Block`. Typical compression ratio is 3-5x.

**Spec reference:** 13.1

---

### SER-002: Header Serialization

Headers MUST be serialized with bincode only (no compression): `L2BlockHeader` -> `bincode::serialize()` -> `CF_HEADERS`. The read path is: `CF_HEADERS` -> `bincode::deserialize()` -> `L2BlockHeader`. Headers are small and frequently read; compression overhead is not worthwhile.

**Spec reference:** 13.2

---

### SER-003: Wire-Format Interop

MUST provide `block_to_wire_bytes(&L2Block) -> Vec<u8>` using `chia-traits::Streamable` for peer gossip compatibility. MUST provide `block_from_wire_bytes(&[u8]) -> Result<L2Block>` for the reverse direction. The wire format differs from the storage format (Streamable vs bincode).

**Spec reference:** 13.3

---

### SER-004: Round-Trip Guarantees

`bincode::deserialize(bincode::serialize(x))` MUST equal `x` for all stored types. `zstd::decode_all(zstd::encode_all(x))` MUST equal `x`. Dictionary-compressed blocks MUST decompress identically to the original pre-compression bytes. `block.hash()` MUST be invariant across serialize/deserialize cycles.

**Spec reference:** 13.4

---

### SER-005: Dictionary Training and Management

On first startup with no dictionary, the store MUST operate in plain-zstd mode. After 1000 blocks are stored, the store MUST train a dictionary via `zstd::dict::from_samples()` on a random sample of block bodies. The dictionary (~100KB) MUST be persisted to `CF_METADATA` under the key `META_ZSTD_DICT`. Subsequent startups MUST load the dictionary from metadata. If dictionary decompression fails, the store MUST fall back to plain zstd (to handle pre-dictionary blocks).

**Spec reference:** 13.1
