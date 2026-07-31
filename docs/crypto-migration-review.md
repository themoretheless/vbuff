# Crypto primitive migration: independent review

Independent verification of wave B item 2 (theme T5): the migration of the
workspace's HMAC, signed-chain and replay-window mechanisms onto three shared
primitives.

Companion to `docs/domain-separation-convention.md`, which supplied the
inventory of 42 preimage builders used here as a checklist. Every claim below
was re-derived from the code, not copied from that document.

**Reviewer wrote no Rust.** This review is read-only; the working tree contains
only the migrating agents' changes.

## State this review was made against

* Base commit `3773bad` ("Record wave B1 closure and the confirmed
  search-grammar defects").
* Working tree **uncommitted**, 14 modified files plus 3 new files:
  `crates/vbuff-types/src/mac.rs`, `crates/vbuff-ipc/src/replay.rs`,
  `crates/vbuff-sync/src/chain.rs`.
* The tree was verified stable (no `.rs` mtime change for 75 s) before the
  test and clippy runs below. Findings were re-read against this final state;
  earlier intermediate states are not described.

## What was checked

14 mechanisms in direct scope, plus a sweep of every other preimage builder
and every authenticator comparison in the workspace for collateral damage.

| Family | Mechanisms | Migrated |
|---|---|---|
| HMAC | 6 | 5 |
| Replay windows | 4 | 3 |
| Signed chains | 3 | 3 |
| Signed receipt | 1 | 1 |

---

## 1. Persistence: touched mechanisms whose bytes leave the process

The rule from `domain-separation-convention.md` §7: any change to the domain,
terminator, field order, encoding or length-prefix width is a format break, and
must either preserve the bytes exactly or bump the domain and make stale data
fail closed.

### 1.1 Bytes preserved exactly (no format break)

All five migrated HMAC mechanisms keep their preimages byte-identical. The
`MacDomain::legacy_unterminated` / `legacy_ascii_separated` constructors exist
precisely to express the pre-existing framing rather than silently normalising
it.

| Mechanism | Domain | Old framing | New expression | Boundary |
|---|---|---|---|---|
| `vbuff-ipc/src/integration/webhook.rs` | `vbuff-webhook-v1` | no terminator | `legacy_unterminated` | **third-party verifiers** |
| `vbuff-ipc/src/api_token.rs` | `vbuff-local-api-v1.` | `.` in the constant | `legacy_ascii_separated(_, b'.')` | issued tokens, 30-day TTL |
| `vbuff-ipc/src/callback.rs` | `vbuff-x-callback-v1.` | `.` in the constant | `legacy_ascii_separated(_, b'.')` | issued tokens, 10-min TTL |
| `vbuff-ipc/src/integration/access.rs` | `vbuff-mcp-lease-v1` | no terminator | `legacy_unterminated` | in-process lease |
| `vbuff-ipc/src/integration/automation/remote.rs` | `vbuff-remote-paste-v1` | no terminator | `legacy_unterminated` | in-process lease |

Verified three ways:

1. **Label and separator concatenation** checked character by character against
   the old byte-string constants. All five reproduce the original bytes.
2. **Field order** checked against the pre-migration `mac.update(..)` sequences.
   `access.rs` (session_id, issued BE u64, expires BE u64, policy_hash,
   consented u8) and `remote.rs` (request_hash, issued BE u64, expires BE u64)
   are unchanged and remain entirely fixed-width, so the absent terminator
   cannot create an ambiguity.
3. **Freeze tests** exist for all five, in `vbuff-ipc/src/{api_token,
   callback}.rs` and `integration/{webhook,access,automation/remote}.rs`. They
   compare against `crate::legacy_mac`, a second hand-rolled HMAC
   implementation kept in `vbuff-ipc/src/lib.rs` under `#[cfg(test)]`. This is
   the right technique: the pin is against an independent implementation, not
   against the primitive checking itself.

Additionally, the three framing vectors pinned in
`crates/vbuff-types/src/mac.rs:217-242` were **recomputed with an independent
Python `hmac`/`hashlib` implementation** and match to the byte:

```
label ‖ 0x00 ‖ "payload"  -> 7c7b29a64ed397ad1eed15572013231057e502d9b94eb2dcf73e62f367795bc7
label ‖        "payload"  -> 407efcbbeb7944725aa3f6841b71e9cd4d7e01de8d55f6bf3dc6c4a18218004e
label ‖ '.'  ‖ "payload"  -> 62149bc09e87ae1fe2451c353440fd833098854d4c12d5136f32309768201f04
```

So `MacDomain`'s three framings are provably what they claim to be.

### 1.2 Format deliberately broken, domain bumped, stale data fails closed

| Mechanism | Domain | Fail-closed guard |
|---|---|---|
| `vbuff-sync/src/ledger.rs` `SyncLedger` | `vbuff-sync-ledger-v1` → **v2** | `#[serde(deny_unknown_fields)]` on `ChainLink`/`LedgerEntry`; test `stale_v1_entries_fail_to_deserialize` |
| `vbuff-sync/src/membership.rs` `MembershipLog` | `vbuff-membership-entry-v2` → **v3** | same; test `stale_v2_entries_fail_to_deserialize` |
| `vbuff-sync/src/provenance.rs` `ProvenanceChain` | `vbuff-custody-v1` → **v2** | same; test `stale_v1_entries_fail_to_deserialize` |
| `vbuff-sync/src/membership.rs` SAS | `vbuff-membership-sas-v2` → **v3** | n/a (interactive) |

This is the correct handling. The chains changed in three independent ways at
once — signature now covers the preimage instead of the bare digest, the
preimage gained NUL termination and `u32_be` length prefixes, and the serde
shape moved the payload under a `payload` key — so silent misinterpretation of
old data had to be made impossible. `deny_unknown_fields` plus the moved key
achieves that: old JSON fails to deserialize rather than verifying under new
rules.

**Confirmed no live persistence writer.** `SyncLedger`, `ProvenanceChain`,
`MembershipLog` and `CapabilityVerifier` have no caller anywhere in `src/` or
`tests/` at the workspace root, and none outside their own crate. They are
`Serialize`-capable and designed to cross a device boundary, but nothing in
this repository writes them to disk yet. The format break is therefore free
today, exactly as `domain-separation-convention.md` §6.2 rated the merkle fix.
This is the single most important mitigating fact about this wave and it should
be re-checked before any of these types is wired to storage.

### 1.3 Domain separation newly added (security improvement)

`vbuff-sync/src/ledger.rs` `WipeReceipt` previously signed a bare
`serde_json::to_vec(&tuple)` with **no domain at all**. It now signs a
`Preimage` under `vbuff-wipe-receipt-v1`. Two further improvements landed with
it: `device_id` is now validated with `is_valid_identifier`, and the signature
became a fixed `[u8; 64]`.

Correct in substance. See defect **D2** for the one gap.

---

## 2. Constant-time comparison

### 2.1 Result

**No timing-exploitable comparison was introduced, and the migration
strengthened the guarantee structurally.**

Every one of the six HMAC verifications in the workspace is constant-time. The
five migrated ones now reach it through `MacProof::verify`
(`crates/vbuff-types/src/mac.rs:155`), which wraps `Mac::verify_slice`
(`subtle::ConstantTimeEq` internally). The sixth, `vbuff-sync/src/capability.rs:73`,
still calls `verify_slice` directly — safe, but not covered by the type.

The structural improvement is worth naming: `hmac_proof` returns an opaque
`MacProof` rather than `[u8; 32]`. A verifier therefore *cannot* write
`computed == received`; the only comparison available is the constant-time one.
Prior to this, nothing but convention stopped a future edit from introducing a
`==`.

Other correct constant-time paths, unchanged by this wave:

* `vbuff-sync/src/device_experience/travel.rs:128` — hand-rolled
  `constant_time_eq` over the QR bearer token. Correct: length check, then an
  XOR-fold with no early exit.
* `vbuff-update/src/state.rs:141` — `blake3::Hash` comparison, constant-time by
  the type's own implementation.

### 2.2 Non-constant-time comparisons, with exploitability assessment

The workspace has roughly 60 production `==`/`!=` comparisons on digests. The
decisive question is not "is it a digest" but **can the attacker compute the
value themselves**. If they can, a timing leak reveals nothing secret.

| Class | Count | Exploitable | Reasoning |
|---|---|---|---|
| Recomputed-vs-presented digest over attacker-known input | ~50 | **No** | The attacker supplies the body/policy/request and can compute the digest offline. Timing leaks a value they already hold. Covers `webhook.rs` body/endpoint hashes, `callback.rs:222` target hash, `remote.rs:141` request hash, `access.rs:222` policy hash, all chain `previous_hash`/`hash` links, `cas.rs`, `content_hash`, plugin bundle/pack hashes. |
| Public key / public chain data | ~8 | **No** | Ed25519 public keys, lockfile pins, merkle roots. Not secret. |
| Post-AEAD plaintext digest | 1 | **No** | `vault_export.rs:84` runs only after AEAD authentication succeeded. |
| **Secret compared against attacker-supplied bytes** | **2** | **See below** | |

The two that genuinely compare a secret:

**(a) `crates/vbuff-platform/src/wayland.rs:137`** —
`self.challenge == expected_challenge` on `[u8; 16]` inside
`GnomeBridgeHello::compatible`. The hello arrives from the GNOME bridge; the
expected challenge is a locally held secret the bridge must prove knowledge of.
`[u8; 16] == [u8; 16]` lowers to a short-circuiting comparison.

*Assessment: real defect, currently **latent**.* `compatible` has **no
production caller** — the only references are its own unit test at
`wayland.rs:180-181`. This is precisely the situation the merkle collision was
in before wave A: cheap to fix now, and it must be fixed before the GNOME
bridge handshake is wired up. Practical exploitation would also require
resolving a few-nanosecond memcmp difference across a local IPC boundary, which
is hard but not categorically impossible.

**(b) `crates/vbuff-core/src/capture/ledger.rs:51`** —
`nonce == write.nonce`, a `&str` comparison of the self-write echo-suppression
nonce.

*Assessment: not practically exploitable.* The attacker's only oracle is
"was the clipboard event captured or not", the nonce lives for 2 s
(`SelfWriteLedger::default`), and `write.hash == hash` short-circuits ahead of
it. Iterating a byte-at-a-time guess inside that window with no timing readout
is not a realistic path. Worth a comment, not a fix.

### 2.3 Hazard, not a defect

`QrHandoffToken` (`travel.rs:53`) derives `PartialEq` over the raw `[u8; 24]`
bearer token, which bypasses the `constant_time_eq` used in `consume()`. No
production path uses the derived equality today. The same pattern recurs on
roughly 25 structs across the workspace that derive `PartialEq` while holding a
signature or MAC field. None is used for authentication today; all are one
careless `==` away from being.

---

## 3. Domain separation

### 3.1 No collisions found

All 40 production domain literals were extracted from the source and checked
pairwise:

* **No domain is a prefix of another.** The only prefix pairs in the tree
  (`vbuff-a-v1` / `vbuff-a-v11`, `vbuff-update-manifest-v1` /
  `vbuff-update-manifest-v1x`) exist *inside tests*, deliberately, to
  demonstrate why the terminator is required — see
  `mac.rs::the_terminator_is_what_stops_a_prefix_domain_collision`.
* **No two mechanisms share a domain.** The one apparent duplicate,
  `vbuff-natural-query-v1`, resolves to a single builder at
  `vbuff-core/src/recall/query.rs:282`; the other hits are a doc comment and the
  distinct `vbuff-query-ast-v1`.
* **Interpolated domains stay unambiguous.** `vbuff-group-epoch-{epoch}` and
  `vbuff-revoke-v1:{epoch}:{issued_at_ms}` are whole AEAD AAD strings, and
  decimal formatting without leading zeros keeps the `:` separator injective.

### 3.2 New framing is unambiguous by construction

`chain.rs::Preimage` implements §5.1 of the convention document: `DOMAIN ‖ 0x00
‖ field*`, every variable-length field behind a `u32_be` prefix *including the
last*, `u8` discriminants for enums, and `optional()` encoding `None` as `0x00`
versus `Some` as `0x01 ‖ len ‖ bytes`. The three migrated payloads
(`LedgerEntry`, `MembershipEntry`, `CustodyRecord`) were each walked field by
field: no two distinct field tuples can produce the same bytes.

Two details are better than the convention required:

* `Preimage::var` **poisons** on a field exceeding `u32::MAX` and `finish()`
  fails closed, rather than truncating the length into an ambiguity.
* Enum discriminants are explicit `const fn discriminant()` methods, so a
  `#[serde(rename)]` cannot move signed bytes.

### 3.3 The earlier merkle collision is genuinely fixed

Re-verified from code, not from the changelog.
`vbuff-sync/src/merkle.rs::leaf_hash` now emits `vbuff-merkle-leaf-v2\0` and
length-prefixes both `record_id` and `node_id`. The byte-shifting collision
documented in `domain-separation-convention.md` §4.1 is closed, and the domain
bump prevents v1 and v2 trees from being compared.

### 3.4 Remaining no-domain sites (not regressions; pre-existing)

| Site | Convention doc | Status |
|---|---|---|
| `vbuff-core/src/hash.rs::content_hash` | site 13, **P1** | untouched; still no domain, MIME still unprefixed |
| `vbuff-sync/src/capability.rs` | site 35, **P1** | untouched; still no domain |
| `vbuff-store/src/migration.rs::schema_hash` | site 18, P4 | untouched |
| `vbuff-core/src/intelligence/actions.rs` PasteGuard | site 14, P4 | untouched |

---

## 4. Test and lint status

Run against the final stable tree described above.

| Command | Result |
|---|---|
| `cargo check --workspace --all-targets` | clean |
| `cargo test --workspace` | **682 passed, 0 failed, 3 ignored** |
| `cargo clippy --workspace --all-targets` | **clean, zero warnings** |

No regressions. Nothing to localise.

---

## 5. Defects

Ordered by severity. None blocks the migration; all are small.

### D1. The new `vbuff-types` dependency comment is factually wrong

`crates/vbuff-types/Cargo.toml` adds `hmac` and `sha2` with the justification:

> Both are already linked by every consumer of this crate through vbuff-core,
> so this adds no crate to any build graph.

**This is false for `vbuff-platform`.** That crate lists `vbuff-core` only
under `[dev-dependencies]` (`crates/vbuff-platform/Cargo.toml:25-26`); its
normal dependency section has `vbuff-types` but not `vbuff-core`. Adding
`hmac`/`sha2` to `vbuff-types` therefore *does* add two crates to
`vbuff-platform`'s production build graph.

The claim holds for `vbuff-gui` (real dependency at line 10) and the other
consumers. Impact is a slightly larger build, not a security issue — but the
comment asserts a property that was not checked, and the whole point of the
module is that its invariants are checked.

### D2. `WipeReceipt`'s format break has no fail-closed guard

The three chains got `deny_unknown_fields` plus a moved key, so stale data
fails to deserialize. `WipeReceipt` did not. Its serde shape is effectively
unchanged: `signature: Vec<u8>` became `[u8; 64]` via a custom
`serialize`/`deserialize` pair that still reads a 64-element sequence.

Consequence: a receipt issued by an older build **deserializes successfully**
and then fails signature verification, surfacing as
`SyncError::Crypto` — indistinguishable from tampering. Every other format
break in this wave was made to fail as *stale data*. This one fails as
*attack*, which is the misleading direction during a rollout.

Contained today because nothing persists receipts, but `burn.rs::accept_receipt`
consumes receipts from remote devices, so this becomes a real cross-version
issue the moment burn sessions span builds.

### D3. `vbuff-sync/src/capability.rs` was left out entirely

It is the **only HMAC in the workspace with no domain string** (convention doc
site 35, rated P1), and it was the stated motivation for the shared primitive.
After this wave the workspace has one MAC primitive that five of six mechanisms
use, and the sixth is the one that most needed it.

Its `CapabilityVerifier` also keeps two unbounded `BTreeMap`s
(`capability.rs:46-47`). They are pruned by expiry but have **no capacity
ceiling**, which is exactly the property `ReplayGuard` was created to enforce
("the entry count has a hard ceiling and saturation fails closed",
`replay.rs:11`). Severity is limited because entries are only inserted after a
successful signature check, so growth requires the secret — but `revoke()` is a
public method with no bound at all.

Defensible as out of scope (the brief said "HMAC in vbuff-ipc"), but it means
T5 is not closed.

### D4. `ReplayGuard` cannot reach the crate that needs it

`replay.rs` is declared `mod replay;` (private) in `vbuff-ipc/src/lib.rs:11` and
`ReplayGuard` is `pub(crate)`. `vbuff-sync`'s `CapabilityVerifier` therefore
cannot use it even if D3 were addressed, without first promoting the module.
Reasonable as a first step; worth recording so the next wave does not
re-implement it.

---

## 6. Requires a decision

Not defects. Trade-offs the migration made or inherited that someone should
consciously accept.

### R1. The monotonic clock floor is now a permanent-poisoning failure mode for two more mechanisms

`ReplayGuard::advance_to` clamps `now_ms` to a non-decreasing floor. This is
correct and necessary — it is what stops a rewound clock re-opening a closed
window — and `CallbackTokenIssuer` already behaved this way.

The refactor extends it to `RemoteReplayWindow` and `WebhookReplayWindow`,
which previously had no floor. New consequence: a single caller passing an
erroneously large `now_ms` (a clock glitch, a units bug) raises the floor
permanently and every subsequent event is refused as `Expired` for the lifetime
of the process, with no recovery short of restart.

`now_ms` is caller-supplied, not attacker-supplied, so this is not a remote
DoS. But it is a new fail-closed-but-unrecoverable mode for two mechanisms, and
it deserves an explicit decision: accept it, or bound the forward jump.

### R2. The SAS domain bump was the free moment to fix the SAS digit count, and it was missed

`membership.rs:201-202` reduces the digest modulo `10_000_000_000_000_000_000`
— that is **10^19**, not 10^20 — and then formats with `{value:020}`. The
leading digit is therefore **always `0`**, and the real entropy is
log2(10^19) ≈ 63.1 bits, not the ≈ 66.4 bits claimed in the doc comment at
`:184-186` and in the test name at `:580`.

This is **pre-existing** (present unchanged at `3773bad`), so it is not a
regression from this wave. But `SAS_DOMAIN` *was* bumped v2 → v3 in this wave,
which means SAS digits change for mixed-version pairings anyway — this was the
one moment when fixing the constant would have cost nothing extra. It is still
above the 60-bit floor, so it is a decision, not an emergency.

### R3. Three cross-device formats break simultaneously

Ledger, membership and custody all bump domains in one change, and membership
additionally changes the SAS the pairing ceremony displays. Per
`domain-separation-convention.md` §7.2 item 24, a mixed-version pairing shows
**different SAS digits on the two devices**, which trains users to read a
legitimate pairing as an attack.

Free today because nothing persists these types (§1.2). The decision to record
is that they must ship together, in one release, before any of them is wired to
storage or to a real pairing flow.

### R4. `content_hash` remains undomained, and it is the most persisted digest in the product

Convention doc site 13, rated P1. Untouched by this wave, correctly — it needs
a clip-table rehash migration and coordination with the grace-bin identity
check at `lifecycle.rs:316`, which is a wave of its own.

Also still open, and noticed again while reading: `content_hash` hashes inline
bytes while `content_hash_from_flavors` (`hash.rs:49-52`) substitutes
`blob_ref.as_bytes()` for spilled bodies, so the two disagree for any clip with
a spilled flavor despite the doc comment at `:41-43` claiming stability.

---

## 7. Assessment

The migration is **sound and better than the convention document asked for**.

What was done well:

* Byte-compatibility for all five already-issued MAC formats, expressed
  explicitly as `legacy_*` constructors rather than preserved by accident, and
  pinned by freeze tests against an independent implementation.
* Domain bumps on every format that genuinely changed, each with a
  deserialization guard that makes stale data fail as stale rather than as
  tampering — with the single exception of D2.
* `MacProof` makes the constant-time comparison structurally unavoidable rather
  than conventional.
* `Preimage` fails closed on length overflow instead of truncating.
* `ChainEntry::expected_signing_key` runs identically on append and verify, so
  the writer's required key and the reader's checked key cannot drift.

What remains: D1–D4 above, and the P1 items (`content_hash`, `capability.rs`)
that this wave did not claim.
