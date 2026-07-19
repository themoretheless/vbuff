# Data contract v1

Frozen on 2026-07-18 before broader CLI, daemon IPC, and replication consumers attach. The executable oracle is `tests/data_contract_freeze_v1.rs`; this document explains what a deliberate change must preserve. The current schema has advanced without rewriting this record; see [data contract v2](data-contract-v2.md).

## Frozen surfaces

| Surface | v1 value | Compatibility rule |
|---|---|---|
| Store schema | `vbuff_store::DATA_CONTRACT_V1_SCHEMA_VERSION == 5` | A schema change requires a forward migration, prior-version fixture coverage, and a new recorded version. Never rewrite or wipe an unknown store. |
| Content hash | BLAKE3 over the canonical flavor set. The HTML/plain fixture hashes to `db6a06a659f896aadf82f2d907704f2a9800748b5f799b3ee5f577cfec45f783`. | Flavor ordering and MIME normalization must not silently change identity. A new algorithm needs a versioned hash domain and mixed-version behavior. |
| Format keys | Windows `CF_UNICODETEXT` -> `PlainText`; macOS `org.nspasteboard.ConcealedType` -> `Concealed`. | Add mappings compatibly. Reassigning an existing native key requires a new format-contract version and backend migration plan. |
| IPC hello | `{"client_name":"contract-fixture","protocol":{"minimum":1,"maximum":1},"requested":["read_history","subscribe_events"]}` | Field names, enum spelling, capability order, and protocol range are wire behavior. Breaking serde changes require protocol negotiation and an old-reader test. |

The broader byte-fidelity oracle is `crates/vbuff-platform/tests/corpus/format-fidelity-v1.json`. It currently covers CF_HTML with a BOM, RTFD, file promises, Excel OOXML, PNG, HTML image references, GNOME file lists, and custom formats. Every case must map through `canonical_format`, round-trip without byte drift, and report a deliberate mutation as degraded.

## Schema 5 AI boundary

Schema 5 adds fail-closed `ai_allowed` metadata and content-hash-keyed embeddings. Eligible vectors are computed before taking SQLite's write lock and written in the same capture transaction only for non-sensitive, explicitly eligible text. A denied or sensitive re-copy removes every existing vector and search/fingerprint derivative. Legacy rows without affirmative eligibility do not receive migrated embeddings.

The current database is still bundled plaintext SQLite, not SQLCipher. Keeping embeddings in the same database removes a plaintext side-index, but it does not earn an encrypted-at-rest claim. SQLCipher plus OS-keystore keying remains the release gate.

## Change procedure

1. Add the new version or negotiated representation; do not edit the old fixture into passing.
2. Add forward migration and mixed-version reader/writer tests where the surface crosses a process or device.
3. State whether old clients are read-only, rejected, or safely compatible.
4. Update this record, the batch ledger, and the four top-level documents in the same change.
5. Run the full locked workspace tests and `git diff --check` before acceptance.
