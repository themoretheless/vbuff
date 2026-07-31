# Domain separation convention

Audit + proposed convention for every domain-separated hash / MAC / signature
preimage in the workspace.

Origin: `docs/solid-dry-review-2026-07-26.md`, T5 ("плавающая конвенция
NUL-терминатора доменов по workspace (9 билдеров)"), wave A item 5, part T5.

Status: **audit only**. No Rust source was modified while producing this
document. Everything below is a description of the code as of the current
worktree, plus a proposal.

Concurrency caveat: other wave-A work was landing in this worktree while the
audit ran (commits `04c9768`, `21823e3`, plus uncommitted edits in
`vbuff-update`). The `vbuff-update` rows and line numbers were re-verified
after those edits; line numbers elsewhere were captured earlier and may have
drifted by a few lines. The domain strings, field orders and persistence
classifications are unaffected.

Correction to the review: the review counted **9 builders**. The actual count
is **37 builders that carry an explicit `vbuff-…` domain string**, plus **5
preimage builders with no domain at all**, for **42 sites** total across 7
crates. The review's "9" is roughly the count of NUL-terminated domain
constants (`b"vbuff-…\0"`), not the count of preimage builders.

---

## 1. Method

Sites were found by grepping `crates/` and `src/` for:

* the literal `vbuff-` inside byte-string and string literals,
* `blake3::Hasher::new`, `blake3::Hasher::new_keyed`, `blake3::hash(`,
* `Hmac`, `Mac::`, `new_from_slice`, `mac.update`, `hasher.update`,
* `Hkdf::`, `derive_key`,
* `const … _DOMAIN`,
* `.sign(` / `.verify(` to catch signatures over bare digests.

Each hit was read in full to record: the domain string, the terminator (or
absence), whether variable-length fields carry a length prefix, field order,
and whether the resulting value crosses a persistence or trust boundary.

Excluded as *not* preimage builders (bare content hashes with no structure to
be ambiguous about): `vbuff-store/src/cas.rs::file_hash`,
`vbuff-store/src/migration.rs::file_checksum`,
`vbuff-core/src/fingerprint.rs`, `vbuff-core/src/bloom.rs`,
`vbuff-core/src/capture/integrity.rs`, and the many single-argument
`blake3::hash(x.as_bytes())` label hashes in `vbuff-ipc/src/integration/*`.

Notation used in the tables:

* **term.** = what separates the domain from the first field.
  `\0 (const)` = the NUL is baked into the domain constant;
  `\0 (push)` = the constant has no NUL and the code appends `0` explicitly;
  `none` = fields are concatenated straight onto the domain;
  `.` / `:` / `-` = an ASCII punctuation separator.
* **len-prefixed** = every variable-length field carries an explicit length.
* **ambiguity** = can two distinct field tuples produce the same preimage.

---

## 2. Inventory

### 2.1 `vbuff-update`

> **Note.** While this audit was being written, a sibling change landed
> `manifest::signing_preimage` and migrated sites 1, 2 and 4 onto it. The rows
> below describe the code as it now stands; §5.4 reconciles that helper with
> the workspace-wide proposal.

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 1 | `crates/vbuff-update/src/manifest.rs:14`, built at `:428-435` via `signing_preimage` (`:52-71`) | `vbuff-update-manifest-v1` (bare `&str`) | `0x00` appended by the helper | no, `0x00` **separates** parts | domain, `0`, key_id, `0`, canonical JSON | **yes** (signed manifests from the update server) | none |
| 2 | `crates/vbuff-update/src/manifest.rs:15`, built at `:140` | `vbuff-update-key-rotation-v1` (bare `&str`) | `0x00` from the helper | no | domain, `0`, key_id, `0`, canonical JSON | **yes** (rotation proof-of-possession inside a manifest) | none |
| 3 | `crates/vbuff-update/src/manifest.rs:442-450` | `vbuff-staged-rollout-v1` | **none** | n/a | domain, sequence BE u64, installation_id (var, last) | no (bucket is recomputed) | none |
| 4 | `crates/vbuff-update/src/attestation.rs:12`, built at `:82-89` | `vbuff-build-attestation-v1` (bare `&str`) | `0x00` from the helper | no | domain, `0`, key_id, `0`, canonical JSON | **yes** (build attestations) | none |
| 5 | `crates/vbuff-update/src/state.rs:65`, built at `:110-118` | `vbuff-update-verifier-state-v1` | `\0 (const)` | no (JSON last) | domain, schema BE u32, canonical JSON | **yes** (verifier state file) | none |

`key_id` is validated by `vbuff_types::validation::valid_key_id`
(`crates/vbuff-types/src/validation.rs:145`), whose charset is
`[A-Za-z0-9._-]`, so the `0x00` after it is a genuine unambiguous separator and
the resulting bytes are identical to the three historical hand-rolled copies
(pinned by `manifest.rs:794-820` and `attestation.rs:159`).

Two written statements of the convention now exist in this crate, and they
disagree on where the NUL lives. `state.rs:61-65` says:

> Domain separator for the state checksum, matching the crate's convention of a
> NUL-terminated `vbuff-<purpose>-v<n>` prefix on every hashed or signed
> preimage.

while `manifest.rs:25-29` says the opposite and is the newer of the two:

> The domain is a bare ASCII label, never a NUL-terminated constant. The
> terminator belongs to the framing, so it is appended here exactly once.

The newer statement is the better rule (see §5.1 point 2); site 5 has simply
not been migrated to it yet. Site 3 follows neither, deliberately and with a
documented justification at `manifest.rs:437-441`.

### 2.2 `vbuff-plugin`

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 6 | `crates/vbuff-plugin/src/bundle.rs:57` (`reproducible_bytes`) | `vbuff-native-plugin-bundle-v2` | `\0 (const)` | **yes**, BE u64 on every field + BE u64 asset count | domain, manifest, executable, count, (path, bytes)* | **yes** (signed, distributed bundles) | none |
| 7 | `crates/vbuff-plugin/src/snippet_pack.rs:13`, built at `:156-158` | `vbuff-snippet-pack-v1` | `\0 (const)` | n/a (single fixed 32-byte field) | domain, pack_hash[32] | **yes** (`SignedSnippetPack`) | none |
| 8 | `crates/vbuff-plugin/src/offline.rs:9`, built at `:102-108` | `vbuff-offline-run-v1` | `\0 (const)` | no (JSON last) | domain, canonical JSON | **yes** (`SignedOfflineRun`) | none |
| 9 | `crates/vbuff-plugin/src/protocol.rs:112-114` | `vbuff-native-plugin-protocol-v1-len32be-json-pipe` | n/a | n/a | domain only, no fields | n/a (**currently unused outside its own test**) | none |

Site 6 is the reference implementation of a fully unambiguous builder.

### 2.3 `vbuff-core`

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 10 | `crates/vbuff-core/src/privacy.rs:7`, built at `:122-143` | `vbuff-privacy-ledger-v1` | **none** | no (reason last, fixed vocabulary) | domain, prev[32], seq/ts/count BE u64, kind u8, reason (var, last) | no (`VecDeque`, never serialized; see `src/diagnostics.rs:23`) | none |
| 11 | `crates/vbuff-core/src/recall/query.rs:280-307` | `vbuff-natural-query-v1` | **none** | **yes**, BE u32 per string | domain, 4×(len, bytes), kind u8, before i64 BE, after i64 BE | no (explicitly documented at `:309-313` as never persisted) | none |
| 12 | `crates/vbuff-core/src/trust/secrets.rs:12`, built at `:197-220` | `vbuff-detector-update-v1` | `\0 (const)` | **yes**, BE u32 count + BE u32 per id | domain, version/issued/expires BE u64, count, (len, id)* | **yes** (`SignedDetectorUpdate` is distributed) | none |
| 13 | `crates/vbuff-core/src/hash.rs:25-37` (`content_hash`) and `:44-58` (`content_hash_from_flavors`) | **none** | n/a | **partial**: bytes are LE u64 length-prefixed, **the MIME is not** | (mime, len, bytes)* sorted by mime | **yes** (`clips.content_hash`, dedup key, grace-bin identity check) | theoretical, see §4.2 |
| 14 | `crates/vbuff-core/src/intelligence/actions.rs:334-366` (`PasteGuardFingerprint`) | **none** | n/a | **yes**, LE u64 everywhere | count, (len, mime, len, body)* | no (in-memory guard) | none |

### 2.4 `vbuff-store`

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 15 | `crates/vbuff-store/src/lifecycle.rs:221-224` (`normalized_text_fingerprint`) | `vbuff-normalized-text-v1` | `\0 (const)` | n/a (single var field, last) | domain, normalized text | **yes**, and **frozen by a test vector** (`tests/data_contract_freeze_v2.rs:10-16`) | none |
| 16 | `crates/vbuff-store/src/lifecycle.rs:20`, built at `:325-342` (`grace_aad`) | `vbuff-grace-bin-v1` | `\0 (push)` | no, NUL-separated instead | domain, `0`, recovery_id, `0`, clip_id, deleted BE i64, purge BE i64, reason BE i64 | **yes** (AEAD AAD over `grace_bin.ciphertext`) | none (both ids are locally minted `ClipId` reprs) |
| 17 | `crates/vbuff-store/src/data_lifecycle.rs:1184-1189` (`source_fingerprint`) | `vbuff-import-source-v1` | `\0 (const)` | n/a (single var field, last) | domain, source | **yes** (`import_ledger.source_fingerprint`, `crates/vbuff-store/src/lib.rs:624`) | none |
| 18 | `crates/vbuff-store/src/migration.rs:186-207` (`schema_hash`) | **none** | n/a | no, `0` between fields + `0xFF` between records | (kind, `0`, name, `0`, sql, `0xFF`)* | in the migration manifest only | none in practice (`sqlite_master.sql` cannot carry NUL) |

Site 16 is the only place in the tree that uses a NUL as a *field* separator
rather than only as a domain terminator.

### 2.5 `vbuff-sync`

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 19 | `crates/vbuff-sync/src/ledger.rs:133-144` (`ledger_hash`) | `vbuff-sync-ledger-v1` | **none** | **no** | domain, prev[32], signer_device (var, **unvalidated, unprefixed**), event JSON | **yes** (`SyncLedger` is `Serialize`/`Deserialize`) | theoretical, see §4.3 |
| 20 | `crates/vbuff-sync/src/merkle.rs:62-71` (`leaf_hash`) | `vbuff-merkle-leaf-v1` | **none** | **no** | domain, digest[32], record_id (var), physical_ms LE u64, logical LE u64, node_id (var) | wire (reconciliation) | **PRACTICAL, see §4.1** |
| 21 | `crates/vbuff-sync/src/merkle.rs:73-85` (`range_hash`) | `vbuff-merkle-node-v1` | **none** | n/a (two fixed 32-byte fields) | domain, left[32], right[32] | wire | none |
| 22 | `crates/vbuff-sync/src/provenance.rs:137-143` (`event_hash`) | `vbuff-custody-v1` | **none** | no (JSON last) | domain, prev[32], event JSON | **yes** (`ProvenanceChain` is `Serialize`) | none |
| 23 | `crates/vbuff-sync/src/membership.rs:409-419` (`entry_hash`) | `vbuff-membership-entry-v2` | **none** | no (JSON last) | domain, prev[32], JSON tuple `(action, added_by, clock)` | **yes** (membership log) | none |
| 24 | `crates/vbuff-sync/src/membership.rs:232-254` (`sas`) | `vbuff-membership-sas-v2` | **none** | n/a (three fixed 32-byte fields) | domain, head[32], lower key[32], higher key[32] | interactive (both peers must agree) | none |
| 25 | `crates/vbuff-sync/src/membership.rs:377` | `vbuff-group-epoch-{epoch}` | `-` (interpolated) | n/a | AEAD AAD, epoch as decimal, last | **yes** (wrapped group keys) | none |
| 26 | `crates/vbuff-sync/src/device_experience/revocation.rs:56` | `vbuff-revoke-v1:{epoch}:{issued_at_ms}` | `:` (interpolated) | n/a | AEAD AAD, two decimals | **yes** (sealed revocation envelopes) | none |
| 27 | `crates/vbuff-sync/src/crypto.rs:99-102` (`seal_for_relay`) | `vbuff-relay-route-v1` | **none** | n/a | keyed BLAKE3 (routing secret as key), domain, epoch LE u64 | wire | none |
| 28 | `crates/vbuff-sync/src/crypto.rs:185-186` (`derive_sealed_key`) | `vbuff-sealed-sender-v1` | n/a | n/a | HKDF **info**, domain only | wire | none |
| 29 | `crates/vbuff-sync/src/vault_export.rs:9`, built at `:167-174` | `vbuff-portable-vault-v1` | **none** | n/a (all fixed) | domain, created BE u64, count BE u64, hash[32] | **yes** (portable vault files) | none |
| 30 | `crates/vbuff-sync/src/collection_vault.rs:104` | `vbuff-collection-vault-v1` | n/a | n/a | HKDF **salt**; info = id (var) ‖ epoch BE u64 | **yes** (key derivation for encrypted collections) | none |
| 31 | `crates/vbuff-sync/src/collection_vault.rs:128` | `vbuff-isolated-collection-v1` | n/a | n/a | HKDF **salt**; info = id (var) ‖ epoch BE u64 | **yes** | none |
| 32 | `crates/vbuff-sync/src/bootstrap.rs:15` | `vbuff-bootstrap-v1` | n/a | n/a | AEAD AAD, constant | **yes** (encrypted bootstrap snapshots) | none |
| 33 | `crates/vbuff-sync/src/bootstrap.rs:31` | `vbuff-recovery-v1` | n/a | n/a | BIP-39 seed **passphrase** | **yes** (recovery phrase to root key) | none |
| 34 | `crates/vbuff-sync/src/bootstrap.rs:34` | `vbuff-group-membership-root-v1` | n/a | n/a | HKDF **info** | **yes** | none |
| 35 | `crates/vbuff-sync/src/capability.rs:31-42`, `:71-74` | **none at all** | n/a | n/a | HMAC-SHA256 over bare `serde_json(CapabilityScope)` | wire (capability tokens) | none today, but it is the sole mechanism with no domain, see §4.4 |

### 2.6 `vbuff-ipc`

| # | Site | Domain | Term. | Len-prefixed | Field order | Persisted | Ambiguity |
|---|------|--------|-------|--------------|-------------|-----------|-----------|
| 36 | `crates/vbuff-ipc/src/callback.rs:276`, `:301` | `vbuff-x-callback-v1.` | `.` (in const) | n/a | domain, base64url payload (last) | issued tokens, bounded TTL | none (`.` is outside the base64url alphabet) |
| 37 | `crates/vbuff-ipc/src/api_token.rs:132`, `:169` | `vbuff-local-api-v1.` | `.` (in const) | n/a | domain, base64url payload (last) | issued tokens, bounded TTL | none |
| 38 | `crates/vbuff-ipc/src/integration/webhook.rs:117`, `:128` | `vbuff-webhook-v1` | **none** | n/a (JSON last) | domain, event JSON | **yes** (delivered to third parties) | none |
| 39 | `crates/vbuff-ipc/src/integration/access.rs:16`, built at `:238-268` | `vbuff-mcp-lease-v1` | **none** | n/a (all fixed) | domain, session_id[16], issued BE u64, expires BE u64, policy_hash[32], consented u8 | in-process lease | none |
| 40 | `crates/vbuff-ipc/src/integration/automation/remote.rs:157`, `:174` | `vbuff-remote-paste-v1` | **none** | n/a (all fixed) | domain, request_hash[32], issued BE u64, expires BE u64 | in-process lease | none |
| 41 | `crates/vbuff-ipc/src/integration/automation/snippets.rs:11`, built at `:58-76` | `vbuff-snippet-manifest-v1` | `\0 (const)` | **yes**, LE u64 count + LE u64 per key | domain, count, (len, key, tag u8, hash?)* | **yes** (`cursor.last_manifest_hash`) | none |

Site 41 is the second reference implementation, though it uses **LE** u64
lengths where site 6 uses **BE**.

### 2.7 Signatures over bare digests

Four places sign a raw 32-byte digest with no outer domain:

* `crates/vbuff-sync/src/ledger.rs:61` - `signing_key.sign(&hash)`
* `crates/vbuff-sync/src/membership.rs:136` - `author_key.sign(&hash)`
* `crates/vbuff-sync/src/provenance.rs:78` - `key.sign(&hash)`
* `crates/vbuff-plugin/src/bundle.rs:77` - `signing_key.sign(&bundle_hash)`

All four are currently safe, because each digest's *preimage* already carries a
distinct domain (sites 19, 23, 22, 6). The hazard is the pattern, not the code:
`crates/vbuff-plugin/src/snippet_pack.rs:156-158` does the opposite (the domain
is applied **outside**, at signing time, over a `pack_hash` that has no inner
domain). A future digest built without an inner domain, signed with the
"sign the hash directly" pattern, is an immediate cross-protocol forgery hole
with nothing in the type system to catch it.

---

## 3. How many conventions are actually in use

Eight distinct domain/terminator conventions:

| Convention | Sites | Count |
|---|---|---|
| C1. `b"vbuff-…-v1\0"` constant, fields concatenated raw | 5, 6, 7, 8, 12, 15, 17, 41 | 8 |
| C2. bare `&str` domain, `0x00` appended by a shared helper and `0x00` between parts | 1, 2, 4 | 3 |
| C3. domain literal, no terminator, all following fields fixed-length | 10, 21, 24, 27, 29, 39, 40 | 7 |
| C4. domain literal, no terminator, a variable-length field follows | 3, 11, 19, 20, 22, 23, 38 | 7 |
| C5. domain constant ending in `.` used as the separator | 36, 37 | 2 |
| C6. domain constant with no NUL + explicit `push(0)` + NUL-separated fields | 16 | 1 |
| C7. domain interpolated into a formatted string (`-`, `:` separators) | 25, 26 | 2 |
| C8. domain lives outside the message (HKDF salt / HKDF info / BIP-39 passphrase) | 28, 30, 31, 32, 33, 34 | 6 |
| C9. no domain at all | 13, 14, 18, 35 | 4 |

And four length-prefix conventions: BE u64 (site 6), LE u64 (13, 14, 41), BE
u32 (11, 12), and "none" (everywhere else).

The review's claim that the terminator convention floats is correct. It
understates the spread: the *placement* of the domain floats too (inside the
message, outside as a signing wrapper, or in the KDF salt/info).

---

## 4. Ambiguity analysis

An ambiguity exists when two distinct field tuples serialize to the same
preimage byte string. Fixed-width fields cannot be ambiguous. A single
variable-length field in the final position cannot be ambiguous. Ambiguity
needs at least one variable-length unprefixed field that is *not* last, or two
adjacent unprefixed variable-length fields.

### 4.1 PRACTICAL: `merkle.rs::leaf_hash` (site 20)

`crates/vbuff-sync/src/merkle.rs:62-71`:

```rust
fn leaf_hash(record: &MerkleRecord) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-merkle-leaf-v1");
    hasher.update(&record.digest);
    hasher.update(record.record_id.as_bytes());
    hasher.update(&record.clock.physical_ms.to_le_bytes());
    hasher.update(&record.clock.logical.to_le_bytes());
    hasher.update(record.clock.node_id.as_bytes());
    *hasher.finalize().as_bytes()
}
```

Two variable-length unprefixed fields (`record_id`, `node_id`) with two
fixed-width fields sandwiched between them. `MerkleRecord`
(`crates/vbuff-sync/src/merkle.rs:7-12`) is `Deserialize`, and
`MerkleTree::new` (`:21-32`) applies **no validation whatsoever** to
`record_id` or `node_id`, so both are fully peer-controlled `String`s and may
contain a `\u{0000}` code point.

Shifting the `record_id`/clock boundary by one byte gives a collision:

```
A: record_id = "r\u{0}", physical_ms = 1,   logical = 0, node_id = "node"
B: record_id = "r",      physical_ms = 256, logical = 0, node_id = "\u{0}node"
```

Byte-by-byte after the shared `b"vbuff-merkle-leaf-v1" || digest || "r"`:

```
       record_id tail | physical_ms LE64        | logical LE64            | node_id
A:  00                | 01 00 00 00 00 00 00 00 | 00 00 00 00 00 00 00 00 | 6e 6f 64 65
B:                      00 01 00 00 00 00 00 00 | 00 00 00 00 00 00 00 00 | 00 6e 6f 64 65
```

Both streams are the same 21 bytes: `00 01` followed by fifteen `00` bytes,
then `6e 6f 64 65`. Identical preimage, identical leaf hash.

Generalized: for any `p < 2^56` and `l < 2^56`, the record
`(record_id ‖ "\0", p, l, node_id)` and the record
`(record_id, p<<8, l<<8, "\0" ‖ node_id)` hash identically. Small, entirely
realistic clock values suffice, so this is not a "needs 2^64-byte inputs"
theoretical result.

Impact: `MerkleTree::root()` and `differing_indices()` are the reconciliation
primitive for offline-device catch-up. Two peers holding *different* records
compute the same root and the same leaf hash, so `differing_indices` returns
nothing for that leaf and the divergent record is never reconciled. A peer can
therefore make a record invisible to sync while appearing fully in sync.

Honest caveat: `MerkleTree::new` currently has no caller outside
`crates/vbuff-sync/src/merkle.rs`'s own unit tests (`:120-121`). The defect is
latent today. It must be fixed before merkle reconciliation is wired into a
real sync path, and it is cheap to fix now because nothing persists these
hashes yet.

### 4.2 THEORETICAL: `hash.rs::content_hash` (site 13)

`crates/vbuff-core/src/hash.rs:29-35` hashes, per flavor,
`mime ‖ LE64(bytes.len()) ‖ bytes`. The MIME is variable-length and
unprefixed, sitting directly against a length field. Structurally that is the
same defect class as §4.1.

It is not practically exploitable. Shifting the `mime`/length boundary by `k`
bytes forces the shifted `LE64` to absorb MIME bytes into its low-order
positions, so the alternative length becomes roughly `len << 8k` while the
alternative payload length differs by only `k`. Solving the two constraints
simultaneously requires payload sizes on the order of `2^56`-`2^64` bytes,
which no real clipboard flavor reaches, and `valid_mime`
(`crates/vbuff-store/src/data_lifecycle.rs:1173-1182`) further restricts MIME
strings to ASCII-graphic bytes on the store's import path.

The real finding here is not the ambiguity but that **`content_hash` has no
domain at all**. It is a bare BLAKE3 over structured data, so it shares a hash
space with every other bare `blake3::hash(..)` in the tree. It is also the
most heavily persisted digest in the product (`clips.content_hash`, the dedup
key, and the grace-bin identity check at
`crates/vbuff-store/src/lifecycle.rs:316`).

Adjacent defect noticed while reading, out of T5 scope but worth a ticket:
`content_hash` hashes inline bytes while `content_hash_from_flavors`
(`crates/vbuff-core/src/hash.rs:49-52`) substitutes `blob_ref.as_bytes()` for
spilled bodies. The two functions therefore disagree for any clip with a
spilled flavor, despite the doc comment at `:41-43` claiming stability.

### 4.3 THEORETICAL: `ledger.rs::ledger_hash` (site 19)

`crates/vbuff-sync/src/ledger.rs:138-143` concatenates the unvalidated
`signer_device` string directly onto `serde_json::to_vec(event)`.

Not exploitable in practice: the trailing field is a complete JSON object, and
a suffix of that object that is *itself* a complete valid `SyncEvent`
serialization does not exist (the only nested `{` is `clock`, whose suffix
carries an unbalanced trailing `}` and the wrong field names). Verification
also requires `signer_device` to be a key in the trusted-key map
(`:83-85`), which constrains it after the fact.

Still worth fixing: `SyncLedger::append` (`:52-58`) accepts any
`impl Into<String>` with no `is_valid_identifier` check, unlike
`membership.rs:266`, which does validate `added_by`. That asymmetry is the
same divergence the review flags elsewhere in T5.

### 4.4 NOT AMBIGUOUS, BUT THE OUTLIER: `capability.rs` (site 35)

`crates/vbuff-sync/src/capability.rs:34-36` and `:71-73` compute
`HMAC-SHA256(secret, serde_json(scope))` with **no domain string**. Every
other HMAC mechanism in the workspace (sites 36, 37, 38, 39, 40) prefixes a
domain. There is no ambiguity within `CapabilityScope` itself, but the moment
the capability secret is shared with, or derived from the same material as,
any other HMAC context, a token becomes cross-context replayable. It is the
one mechanism with zero defence in depth.

### 4.5 Everything else

The remaining 37 sites have no ambiguity: their variable-length fields are
either length-prefixed, unambiguously terminated, or last in the preimage.
Sites 3, 10, 11, 21, 22, 23, 24, 27, 29, 38, 39, 40 in particular are safe
*by field layout*, not by convention, which is exactly why a written
convention is needed: the safety is invisible at the call site and is one
appended field away from being lost.

---

## 5. Recommended convention

### 5.1 The rule

Every hashed, MAC'd, or signed preimage in the workspace is built as:

```
preimage := DOMAIN ‖ 0x00 ‖ field*
field    := fixed_width_bytes
          | u32_be(len) ‖ bytes          // variable-length
count    := u32_be(n)                     // before any repeated group
```

with:

1. **Domain string.** ASCII, of the form `vbuff-<purpose>-v<n>`, declared as a
   `const … : &[u8]` at module top, *without* the NUL:
   `const FOO_DOMAIN: &[u8] = b"vbuff-foo-v1";`
2. **Terminator.** A single `0x00` emitted by the shared builder, never baked
   into the constant. Baking it in has produced two of the eight conventions
   (C1 vs C2) and makes the double-terminator sites look accidental.
3. **Length prefix.** Every variable-length field gets `u32_be(len)`. `u32`
   because no field in this workspace legitimately exceeds 4 GiB and the caps
   are all far below that; big-endian because the crate already mixes BE and
   LE and BE is the majority (sites 1-5, 6, 11, 12, 16, 29, 30, 31, 39, 40).
   The rule applies even to the last field: "last field needs no prefix" is
   true but fragile, and it is the exact reasoning that made §4.1 exploitable
   when a field was later appended.
4. **Counts.** Any repeated group is preceded by `u32_be(n)`.
5. **Field order.** Fixed, documented in a doc comment above the builder, and
   never reordered without a domain bump.
6. **Enums.** Encoded as an explicit `u8` discriminant with a documented
   mapping, as `privacy.rs:136-140` and `snippets.rs:66-72` already do. Never
   as the `Serialize` output of a renameable variant.
7. **Signatures.** Sign the *domain-separated preimage*, or a digest whose
   preimage was domain-separated. Never both, and never neither. Prefer
   signing the preimage directly, which removes the `sign(&hash)` pattern of
   §2.7 entirely.
8. **KDF and AEAD.** The domain belongs in the HKDF `info` (not the salt) and
   in the AEAD `aad`, and follows the same `DOMAIN ‖ 0x00 ‖ field*` shape.
   Using the salt for the domain (sites 30, 31) hides it from the fields and
   makes it easy to forget when a second `expand` is added.
9. **One builder.** The whole convention should be one small helper, so that a
   call site cannot express a non-conforming preimage:

   ```rust
   // sketch only, not proposed code
   pub struct Preimage { buf: Vec<u8> }
   impl Preimage {
       pub fn new(domain: &[u8]) -> Self;   // pushes domain, then 0x00
       pub fn fixed(self, bytes: &[u8]) -> Self;
       pub fn var(self, bytes: &[u8]) -> Self;   // u32_be length prefix
       pub fn count(self, n: usize) -> Self;
       pub fn u8(self, v: u8) -> Self;
       pub fn u64(self, v: u64) -> Self;    // big endian
       pub fn finish(self) -> Vec<u8>;
   }
   ```

   Pairing it with `hmac_proof(domain, key, parts)` from the T5 remedy closes
   the sign/verify duplication at the same time.

### 5.2 Why NUL rather than a length-prefixed domain

A length-prefixed domain would be marginally more uniform, but every existing
`\0`-terminated site (8 of them, several with frozen on-disk formats) would
have to change. NUL costs one byte, is already the majority terminator among
sites that have one, and is unambiguous because no `vbuff-…-v<n>` domain
contains a NUL. Keeping NUL means 8 sites need zero format change.

### 5.2b Reconciling with the landed `signing_preimage`

`crates/vbuff-update/src/manifest.rs:52-71` already implements a shared
framing:

```text
domain || 0x00 || parts[0] || 0x00 || parts[1] || … || parts[n-1]
```

It agrees with §5.1 on points 1 and 2 (bare-label domain constant, exactly one
terminator emitted by the framing, mandatory even with no parts) and its
doc comment at `:25-46` is the clearest statement of the rule in the tree.
It differs on point 3: it uses `0x00` as a *field separator* instead of length
prefixes, and documents the resulting caller obligation ("only the final part
may contain NUL bytes"), enforced by a `debug_assert!` at `:53-58`.

That trade is correct **for the update crate** and should not be revisited:
length prefixes would have changed the signed bytes of three persisted,
externally-verified formats (§7.2), and the pinned tests exist precisely to
stop that.

It is not the right rule to generalize, for three reasons:

* The obligation is enforced only by `debug_assert!`, which is compiled out in
  release builds. A release binary will happily produce an ambiguous preimage.
* It cannot express a repeated group, so builders like site 6, 12 or 41 cannot
  use it without losing their count framing.
* It cannot express two variable-length NUL-permitting fields, which is exactly
  the shape that makes §4.1 exploitable.

Proposal: keep `signing_preimage` as-is for the update crate and rename it to
signal its scope (for example `nul_framed_preimage`), and introduce the
length-prefixed `Preimage` builder of §5.1 point 9 as the default for
everything else, including every new builder and every §6.2 migration. Where a
call site genuinely has only "validated identifier(s) followed by one trailing
payload", the two produce equally unambiguous bytes and either is acceptable.

### 5.3 Why `u32_be` rather than `u64_le`

Site 6 uses BE u64, sites 13/14/41 use LE u64, sites 11/12 use BE u32. Any
choice breaks someone. BE u32 matches the two sites whose formats are hardest
to migrate for other reasons (site 12 is distributed, site 11 is in-memory) and
matches the dominant endianness elsewhere in the preimages. Sites 6 and 41
keep their current width under a domain bump if and when they are otherwise
touched; they are already unambiguous, so there is no urgency (see §6).

---

## 6. Conformance

### 6.1 Already conforming in substance (no change required)

Unambiguous, domain-separated, and safe. Cosmetic deviations only (`\0`
placement, length width, endianness). **Do not touch these for tidiness alone;
several are persisted.**

* Site 6 `bundle.rs::reproducible_bytes` (BE u64 prefixes instead of BE u32)
* Site 41 `snippets.rs::compute_hash` (LE u64 prefixes instead of BE u32)
* Site 12 `secrets.rs::update_signing_bytes` (already BE u32 prefixes, already NUL)
* Sites 1, 2, 4 `manifest.rs` / `attestation.rs` (now share `signing_preimage`; see §5.2b)
* Site 5 `state.rs::state_checksum` (only deviation is the NUL baked into the
  constant, which contradicts the newer `manifest.rs:25-29` rule; free to
  align **only** by keeping the emitted bytes identical)
* Sites 7, 8, 15, 17 (single trailing variable field under a NUL-terminated domain)
* Sites 21, 24, 27, 29, 39, 40 (all fields fixed-width)
* Sites 36, 37 (`.` separator against a base64url payload)
* Sites 25, 26, 32, 33, 34, 28 (constant AAD / HKDF info)

### 6.2 Must migrate, in priority order

| Priority | Site | Defect | Format break |
|---|---|---|---|
| **P0** | 20 `merkle.rs::leaf_hash` | practical collision (§4.1) | wire, but **no live caller**, so free today |
| **P1** | 35 `capability.rs` | no domain at all (§4.4) | **wire break**, invalidates issued tokens |
| **P1** | 13 `hash.rs::content_hash` | no domain, MIME unprefixed (§4.2) | **persisted, see §7** |
| **P2** | 19 `ledger.rs::ledger_hash` | `signer_device` unprefixed and unvalidated (§4.3) | **persisted chain, see §7** |
| **P2** | §2.7 `sign(&hash)` sites | convention hazard, opposite to `snippet_pack.rs` | none if only the helper changes |
| n/a | 3 `manifest.rs::rollout_bucket` | deviates from the crate rule, but **deliberately and with a written justification** at `manifest.rs:437-441`; changing it reshuffles every installation's rollout bucket. Leave it. | would move every bucket |
| **P3** | 16 `lifecycle.rs::grace_aad` | C6, the only NUL-as-field-separator site | **persisted AAD, see §7** |
| **P3** | 22, 23, 38 | domain with no terminator before a JSON field | **persisted, see §7** |
| **P4** | 10, 11, 14 | in-memory, free to change | none |
| **P4** | 18 `migration.rs::schema_hash` | no domain | manifest-local |
| **P4** | 9 `protocol.rs::protocol_hash` | unused; either wire it up or delete it | none |

---

## 7. WARNING: persistent formats cannot be changed without a domain bump

**Read this before touching any builder in the list below.** These digests,
MACs, and AAD strings are written to disk, stored in SQLite, embedded in
signed artifacts, or exchanged with another device or process. Changing the
preimage layout without bumping the domain version (`-v1` to `-v2`) does not
produce a clean failure: it produces a *silent mismatch* that the code reads as
tampering, corruption, or an unknown peer.

The rule: **any change to the domain string, the terminator, the field order,
the field encoding, or the length-prefix width is a format break.** Bump the
domain (`vbuff-foo-v1` to `vbuff-foo-v2`), and provide either dual-verification
during a transition window or an explicit migration. Never change the layout
under an unchanged domain.

### 7.1 On-disk / in-database

| Site | Where it lives | What breaks on an unversioned change |
|---|---|---|
| 13 `content_hash` | `clips.content_hash` column | Every stored clip's hash stops matching a recomputation. Dedup silently stops deduplicating (every existing clip looks new). The grace-bin identity check at `lifecycle.rs:316` fails, so **every restore from the grace bin errors with `Corrupt("grace-bin payload identity check failed")`**. Requires a full rehash migration of the clip table. |
| 15 `normalized_text_fingerprint` | correlation tokens; **frozen test vector** at `tests/data_contract_freeze_v2.rs:10-16` | The freeze test fails immediately (that is the point of it). Any persisted correlation token stops matching. This vector is a deliberate data contract: treat it as immutable. |
| 16 `grace_aad` | AEAD AAD over `grace_bin.ciphertext` | **Every row already in the grace bin becomes permanently undecryptable.** AAD mismatch is an authentication failure, not a recoverable error. Users lose all pending "recently deleted" clips with no fallback. |
| 17 `source_fingerprint` | `import_ledger.source_fingerprint` (`crates/vbuff-store/src/lib.rs:624`) | Import dedup/idempotency breaks: a previously imported source re-imports as new. |
| 5 `state_checksum` | update verifier state file | The state file fails its integrity check. `decode_state` is fail-closed by design (`state.rs:25-41`), so the process **refuses to start the updater** rather than resetting the watermark. Note this site already has its own `schema` field, which is the correct escape hatch: bump `VERIFIER_STATE_SCHEMA`, not just the domain. |
| 41 `SnippetSyncManifest::compute_hash` | `cursor.last_manifest_hash` | The mirror cursor no longer matches the manifest, so the next sync is treated as a full divergence. Recoverable but noisy. |
| 30, 31 `collection_vault` HKDF salts | key derivation for encrypted collections | **Derived keys change, so every existing encrypted collection becomes unreadable.** There is no recovery path: the ciphertext is fine, the key is simply gone. |
| 33, 34 `bootstrap.rs` recovery passphrase and root HKDF info | recovery phrase to group-membership root key | **Every recovery phrase a user has written down stops working.** This is the single most destructive item on the list. |
| 32 `BOOTSTRAP_AAD` | encrypted bootstrap snapshots | Existing snapshots fail to decrypt. |
| 29 `vault_export` AAD | portable vault files | Previously exported vault files fail to import. |

### 7.2 Cross-device / signed-artifact formats

| Site | Boundary | What breaks |
|---|---|---|
| 1, 2, 4 `vbuff-update` signing preimages | manifests and attestations produced by the release pipeline and verified by every shipped client | An old client cannot verify a new manifest and a new client cannot verify an old one. Since the manifest carries the update mechanism itself, **a mismatch bricks the update channel**: clients cannot fetch the build that would fix them. Any change needs an overlap release that verifies both domains. |
| 6, 7, 8 `vbuff-plugin` bundle / snippet-pack / offline-run signatures | signed artifacts distributed to users | All previously signed bundles and packs fail verification and must be re-signed. Third-party plugin authors must re-sign. |
| 12 `SignedDetectorUpdate` | detector updates distributed to clients | Clients reject the update as `InvalidSignature`, i.e. **detector updates stop flowing**, which is a fail-open security degradation over time. |
| 19 `SyncLedger` | serialized ledger shared between devices | The chain-hash recomputation at `ledger.rs:77` fails, so `verify` returns `"sync ledger hash chain is broken"` for the **entire history**, not just new entries. There is no partial acceptance. Requires a chain-restart migration. |
| 22 `ProvenanceChain` | serialized custody chain | Same failure mode: the whole chain fails verification. |
| 23 `membership entry_hash` | membership log, shared between all group devices | Whole log fails verification. A device with an old build and a device with a new build cannot agree on group membership, which cascades into key-wrapping failures. |
| 24 `membership sas` | interactive pairing ceremony | The two devices display **different SAS digits**, so users are trained to see a legitimate pairing as an attack. Only affects mixed-version pairings, but the user-visible failure is alarming. |
| 25 group-epoch AAD, 26 revocation AAD | sealed envelopes to member devices | Recipients cannot open the envelope: an epoch transition or a revocation silently fails to apply. A revocation that fails to apply is a security regression. |
| 20, 21 merkle leaf/node | reconciliation between peers | Peers compute different roots and conclude that *every* record differs, causing a full resync. Noisy but not destructive, and **there is no live caller today**, which is why §6.2 rates this free to fix now. |
| 27 relay routing tag, 28 sealed-sender HKDF info | relay transport | Relayed envelopes are not routed / not openable. |
| 35 `capability.rs` | capability tokens | Adding a domain **invalidates every issued token**. Tokens are one-shot and expiring, so the blast radius is one token lifetime, but issuer and verifier must be upgraded together. |
| 38 webhook signature | HMAC delivered to third-party endpoints | **Third-party verifiers break.** Their code is outside this repo and cannot be migrated by us. Requires a versioned signature header and a deprecation window, not a silent change. |

### 7.3 Short-lived (a change costs one token lifetime)

Still a break, but self-healing within the TTL and contained to this process:

* 36 `callback.rs` x-callback tokens (`MAX_TOKEN_TTL`-bounded)
* 37 `api_token.rs` local API tokens (TTL-bounded, `MAX_TOKEN_TTL_MS`)
* 39 `access.rs` MCP lease proofs (in-process lease)
* 40 `remote.rs` remote-paste proofs (in-process lease)
* 18 `migration.rs::schema_hash` (compared only against a manifest produced in
  the same dry run; a change invalidates any manifest carried across the
  upgrade)

### 7.4 Free to change (memory-only)

Verified to have no persistence path:

* 10 `privacy.rs::ledger_hash` - `VecDeque` behind an `Arc<Mutex<..>>`
  (`src/diagnostics.rs:23`); only a `PrivacyLedgerSummary` is ever surfaced.
* 11 `recall/query.rs::fingerprint` - documented at `query.rs:309-313` as
  never persisted; the doc comment there already states the bump rule
  correctly and is a good model for the others.
* 14 `intelligence/actions.rs::PasteGuardFingerprint` - compared within a
  single paste-guard decision.
* 3 `manifest.rs::rollout_bucket` - recomputed each check, though changing it
  reshuffles which installations are in the staged rollout.
* 9 `protocol.rs::protocol_hash` - no caller outside its own test.

---

## 8. Suggested sequencing

1. **Now, no format cost:** fix site 20 (`merkle.rs::leaf_hash`) while it has
   no live caller, and land the length-prefixed `Preimage` builder plus
   `hmac_proof` from the T5 remedy alongside the already-landed
   `signing_preimage` (§5.2b). Migrate the §7.4 memory-only sites onto the
   builder as the proving ground.
2. **Next, with a domain bump each:** site 35 (`capability.rs`) and site 13
   (`content_hash`, requires a clip-table rehash migration and coordination
   with the grace-bin identity check).
3. **Behind a compatibility window:** site 38 (webhook, needs a versioned
   signature header) and sites 1/2/4 (update channel, needs a dual-verify
   overlap release).
4. **Leave alone unless independently touched:** everything in §6.1, and every
   §7.1 item whose only defect is cosmetic. The migration cost is real and the
   security benefit is zero.
