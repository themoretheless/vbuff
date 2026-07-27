# Data contract v2

Frozen on 2026-07-20 for store schema 6. The executable oracle is `tests/data_contract_freeze_v2.rs`; schema-5 migration and disk behavior are covered in `crates/vbuff-store/tests/integration.rs`. Contract v1 remains unchanged in [data-contract-v1.md](data-contract-v1.md), and the current schema is recorded separately in [data-contract-v3.md](data-contract-v3.md).

## Schema 6 lifecycle additions

| Surface | v2 value | Compatibility rule |
|---|---|---|
| Frozen v2 schema | `vbuff_store::DATA_CONTRACT_V2_SCHEMA_VERSION == 6`; v1 remains recorded as `DATA_CONTRACT_V1_SCHEMA_VERSION == 5`, while the current schema may be newer. | Schema 5 migrates forward through backup, dry-run, row-count/quick-check verification, and rollback. A schema-5 binary rejects schema 6 as newer; it must not downgrade or wipe it. |
| Normalized text fingerprint | BLAKE3 over domain `vbuff-normalized-text-v1\0` and a bounded derived representation. The frozen fixture yields `a81b321fba50d0831a3165e22664db05759f41f6f0b9bf3471c90c87001ce31d`. | Canonical flavors and exact `content_hash` remain untouched. Changing normalization requires a new domain/version, backfill, collision evaluation, and mixed-index behavior. |
| Exact dedup ledger | Each exact re-copy appends `(clip_id, merged_at)`; deletion cascades its events. | The ledger contains no clipboard payload. Aggregation/compaction must preserve reuse count and newest event semantics. |
| Encrypted grace record | XChaCha20-Poly1305 with a random 24-byte nonce; AAD binds domain, recovery id, clip id, deletion/expiry times, and reason. | The store borrows a 256-bit key and never persists it. Restore validates AEAD, clip id, and canonical content hash. Algorithm/envelope changes require a versioned format and old-record reader. |
| Retention rules | One row per content kind plus one sensitive override; age/count/grace are bounded and validated. | A non-zero grace rule without an available key defers rather than hard-deletes. Pinned/favorite rows remain excluded; hard privacy TTL remains a separate boundary. |

## Security boundary

Schema 6 does not make the main SQLite database encrypted. SQLCipher and OS-keystore integration remain release gates. The grace-bin ciphertext is independently authenticated, and disk tests prove a large CAS canary is hydrated before deletion, absent from SQLite/WAL/blob plaintext after cleanup, and recoverable with the correct external key. The resident app does not yet supply that durable key, so current row Undo is a five-second in-memory operation cleared when the popup hides.

## Change procedure

1. Keep the v1 and v2 fixtures; add a new contract rather than editing an old oracle into passing.
2. Add a forward migration fixture with preserved rows and explicit old-reader behavior.
3. Version any normalization or encrypted-envelope change before writing it.
4. Prove preview and committed retention select the same candidates before activating runtime maintenance.
5. Update the batch ledger and four top-level documents, then run the full locked acceptance matrix.
