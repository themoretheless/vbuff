# Data contract v3: schema 7 lifecycle and portability

Frozen on 2026-07-21. The executable contract is [`tests/data_contract_freeze_v3.rs`](../tests/data_contract_freeze_v3.rs). This document records what schema 7 adds without rewriting the schema 5/v1 or schema 6/v2 freezes.

| Surface | Frozen contract | Compatibility rule |
|---|---|---|
| Current store schema | `vbuff_store::SCHEMA_VERSION == 7`; v1 remains 5 and v2 remains 6. | Open migrates forward and rejects a newer unknown schema. Old constants never change to match the current version. |
| Mutable organization | `clip_annotations` owns archive, collection, preferred MIME, and legal hold; `collection_policies` owns bounded age/count/byte policy. | Canonical flavors and content hash are not rewritten by organization changes. Missing annotation rows are backfilled during migration; a missing row after migration is corruption, so direct mutation errors and bulk deletion protects the clip. |
| Archive visibility | Ordinary list, text/fuzzy search, near-duplicate lookup, and pin suggestions exclude archived rows; `ArchiveVisibility::{Active, Archived, All}` is explicit. | Callers must opt into archived data. Upgrade cannot silently surface archived content through an auxiliary recall path. |
| Residency evidence | `clip_residency` stores monotonic `ever_on_disk`, `ever_synced`, and `ever_exported` flags. | A transition may set a flag to true and never clears history. Absence is not proof of memory-only behavior. |
| Blob quarantine | `blob_quarantine` records hash, kind, time, and content-free reason after bounded CAS verification. | Corrupt bytes are not returned as healthy. Repair/removal requires a separate explicit operation. |
| Backup evidence | `backup_state` stores one verified timestamp and 64-hex checksum. | Freshness advances only through `record_verified_backup`; writing an export file alone is not verification. |
| Import isolation | `import_quarantine` stores bounded candidates outside normal history until an explicit `RestoreSelection`. | Staging accepts inline payloads only and validates source, actual byte size, declared size, and content hash. A duplicate restore removes only its quarantine row and does not mutate the live clip. Restore reports restored, deduplicated, and unavailable counts. |
| Attachment manifest | Manifest schema 1 lists MIME, byte size, inline/CAS storage, blob reference, and the actual stored text-index projection. | Manifest generation does not require payload hydration and must not expose payload bytes. Thumbnail/OCR side records are not yet represented. |
| Export envelope | `ExportSchemaVersion::V1 == 1`, `V2 == 2`, and `LATEST == V2`; every envelope carries a compatibility note. V2 serializes the current `Clip`/`ClipMeta` record, not schema-7 lifecycle sidecars. | V1 downgrade succeeds only when the clip's sensitivity, expiry, and sync policy are safely representable; otherwise it is rejected. AI permission downgrades conservatively to denied. Exports require resolved inline payloads and verify actual body/meta/hash bounds. No SQLite export compatibility is claimed. |
| Legal hold | Held clips are excluded from direct/batch delete, clear, clear-all, count cap, and ordinary retention. | Hard privacy expiry retains precedence over legal hold. Temporary session protection is a separate runtime exception that can defer expiry until released; legal hold is not a compliance certification. |
| Sensitive reason | `ClipMeta.sensitivity_reason` is optional and serde-defaulted; storage metadata carries only a typed payload-free reason. | Schema 6/older rows load with `None` and retain generic masking. A reason-bearing clip is normalized to sensitive at the store/import boundary, contradictory portable export is rejected, and no matched secret or detector text enters metadata. |

## Migration invariants

1. Schema 6 migrates in one store migration transaction, creates all six lifecycle side tables, trigger/indexes, and backfills annotation/residency rows for existing clips.
2. Existing clip ids, flavor bytes, content hashes, timestamps, pin/favorite state, expiry, search projections, and CAS refcounts remain intact.
3. Export schema version is independent from SQLite `user_version`; export v2 is not called schema 7.
4. Data contract v2 remains executable at `DATA_CONTRACT_V2_SCHEMA_VERSION == 6` and only asserts that current schema is at least that version.
5. A schema-6 binary must refuse schema 7 as newer; downgrade uses an export plus an older consumer, never an in-place `user_version` rewrite.

## Verification

The v3 fixture inserts a clip, creates a bounded collection policy, assigns and archives the clip, proves ordinary history excludes it, explicitly retrieves archived history, and freezes attachment-manifest schema 1. Store unit/integration tests additionally cover legal-hold precedence, retention preview/apply, strict import body/size/hash validation, duplicate restore without live-record mutation, safe export downgrade, transactional export-residency updates, future/corrupt backup timestamps, actual index-manifest projection, resumable in-process CAS scrub/quarantine, GC dry run, schema 5-to-7 migration without clip loss, and an exact schema 6-to-7 fixture that verifies all six tables, the insert trigger, sidecar backfill, and canonical clip preservation.
