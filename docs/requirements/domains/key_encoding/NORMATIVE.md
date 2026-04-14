# Key Encoding - Normative Requirements

| Field | Value |
|-------|-------|
| **Domain** | Key Encoding |
| **Prefix** | KEY |
| **Spec** | [SPEC.md](../../../resources/SPEC.md) |
| **Schema** | [SCHEMA.md](../../SCHEMA.md) |

---

## Requirements

### KEY-001: Hash Keys (32 bytes)

Hash keys for `CF_BLOCKS`, `CF_HEADERS`, and `CF_ATTESTED` **MUST** be raw 32-byte `Bytes32` values with no prefix.

```
key = block_hash.as_ref() → [u8; 32]
```

No additional framing, length prefix, or encoding layer is applied. The raw 32-byte hash is used directly as the RocksDB key.

**Spec reference:** SPEC Section 4.1

---

### KEY-002: Height Keys (8 bytes, big-endian)

Height keys for `CF_CANONICAL` **MUST** be `u64` encoded as 8-byte big-endian. This ensures ascending lexicographic order matches numeric order (height 1 < height 2 < height 1000).

```
key = height.to_be_bytes() → [u8; 8]
```

Big-endian encoding is required so that RocksDB's default bytewise comparator produces numerically ascending iteration order.

**Spec reference:** SPEC Section 4.2

---

### KEY-003: Epoch Keys (8 bytes, big-endian)

Epoch keys for `CF_CHECKPOINTS` **MUST** be `u64` encoded as 8-byte big-endian, following the same pattern as height keys.

```
key = epoch.to_be_bytes() → [u8; 8]
```

This guarantees that checkpoint epochs iterate in ascending numeric order under RocksDB's default bytewise comparator.

**Spec reference:** SPEC Section 4.3

---

### KEY-004: Metadata Keys (variable UTF-8)

Metadata keys for `CF_METADATA` **MUST** be UTF-8 encoded strings. The well-known metadata key names are: `"tip"`, `"genesis_hash"`, `"min_height"`, `"schema_version"`, `"zstd_dict"`.

```
key = name.as_bytes() → &[u8]
```

Keys are variable-length and are stored as raw UTF-8 byte sequences with no null terminator or length prefix.

**Spec reference:** SPEC Section 4.4
