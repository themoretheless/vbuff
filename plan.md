# vbuff - Implementation Plan

This is the executable, milestone-sequenced build plan for vbuff. It is derived from and subordinate to `architecture.md`, which is canonical for the workspace layout, crate names, the crate stack, the data model, and every technical decision. Where this plan and `architecture.md` ever disagree, `architecture.md` wins; treat such a disagreement as a bug in this document. The decision spine (positioning, pillars, roadmap phases, success metrics, non-goals) is canonical for *what* we ship in which phase and *why*; this plan is canonical for *the order in which an engineer builds it*.

All milestone goals and acceptance criteria below describe the **target** unless a paragraph is explicitly labeled current. They are not evidence that the current binary has passed that milestone.

### Current implementation snapshot

- Native desktop `eframe`/`egui` is the only UI/runtime target. Web/WASM demo, binary, and CI paths are removed.
- Generic `arboard` polling reads text or image content. It cannot prove source application, concealed/private markers, clipboard generation/provenance, complete flavor enumeration, or OS-history exclusion.
- Automatic paste is disabled until a native adapter confirms the intended destination immediately before injection. Eligible non-sensitive selection is copy-only; sensitive copy is blocked without proven OS-history exclusion.
- One-time passwords, private keys, recovery codes, and explicit skipped-capture recovery use an at-most-32-item process-memory lane with hard expiry. It never enters SQLite/import, cannot be pinned or session-protected, and vanishes on exit.
- The schema-v7 SQLite database is unencrypted. Strict security mode may block capture while SQLCipher or required native privacy proof is unavailable.
- UI preferences persist through the root configuration, reduced motion follows the OS while unset, and unknown configuration keys fail validation.
- Migration safety copies are temporary and removed only after the upgraded or next-start live store opens fully and passes `quick_check`; a failed open preserves them. They are not durable rollback backups, and backup-evidence APIs do not create a backup.
- The native executable plugin protocol has bounded length-prefixed JSON framing but is contract-only. Process launch, OS sandboxing, host-side capability enforcement, installation, and clipboard grants are release-gated and inactive.

The current product strategy is dated and evidence-gated in [the 2026 competitive strategy refresh](docs/competitive-strategy-2026.md). Cross-platform reach, polish, private sync, OCR, and MCP are no longer a sufficient wedge by themselves. The initial position is technical work across desktop operating systems: contextual recall, tested source-to-target format fidelity, and a capability-based boundary for sensitive delivery and AI access. Every milestone must move one of those systems forward without weakening truthful capability reporting.

---

## 0. The single-process MVP vs. the full multi-crate workspace

`architecture.md` defines a target Cargo workspace of eleven focused crates plus a root binary:

`vbuff-types`, `vbuff-core`, `vbuff-store`, `vbuff-platform`, `vbuff-daemon`, `vbuff-gui`, `vbuff-ipc`, `vbuff-plugin`, `vbuff-sync`, `vbuff-update`, `vbuff-cli`, and the root `src/main.rs` binary.

The planned shippable MVP is a deliberate **single-process subset** of that workspace. The target MVP links and runs:

- `vbuff-types` (plain data: `Clip`, `Flavor`, `ContentKind`, `ClipMeta`, ids)
- `vbuff-core` (engine: dedup, eviction, retention, capture-gate policy, search, classify)
- `vbuff-store` (currently bundled SQLite schema v7 + FTS5 + migrations + blob CAS + exact/near dedup + lifecycle annotations/quarantine/export + externally keyed grace-bin primitives + eligible local embeddings + WAL; SQLCipher and OS-keystore integration are still required)
- `vbuff-platform` (currently traits, generic `arboard` text-or-image polling/write, and capability decisions; native per-OS proof and confirmed-target paste remain milestone work)
- `vbuff-gui` (eframe popup + settings viewports, the egui hot path)
- `vbuff-update` (signed release contracts and the offline checksum verifier; network fetch/install remains deferred)
- the root binary (`src/main.rs`): single-instance guard, role dispatch, wiring

The later crates are explicitly deferred:

- `vbuff-daemon` - the MVP wires the watcher/store/core/GUI directly inside the root binary on background threads; a dedicated daemon-orchestration crate that owns a tokio runtime, supervised threads, IPC server, and sync sockets is extracted later (M7). The capture watcher still runs on its own dedicated thread in the MVP; what is deferred is the *crate boundary* and the tokio-based service host, not background capture itself.
- `vbuff-ipc` - the framed control-socket protocol and client/server; deferred to M7. Browser/editor/Vim/automation/MCP/launcher/terminal/webhook message contracts are bounded foundations only. The MVP still needs a single-instance guard (bind-or-forward) but its forwarded intent is minimal (just "show popup"), not the full IPC verb surface.
- `vbuff-cli` - the `vbuff` verb surface (`list`/`get`/`copy`/`add`/`search`/`delete`, `--json`, completions); deferred to M8. It is a pure IPC client, so it cannot exist before `vbuff-ipc`.
- `vbuff-sync` runtime integration - the crate contains tested CRDT/HLC, envelope, membership, policy, anti-entropy, recovery, receipt, capability, padding, and device-experience foundations. It stays disconnected and frozen. Only the explicit M9 handoff may activate after M6 demand; replication and transports require a separate M11 promotion.
- `vbuff-plugin` runtime integration - manifests, a bounded native executable protocol, typed transforms, migrations, offline evidence, signed packs, adapters, and curated recipes are contract foundations only. No process host or sandbox is active; execution, installation, publisher trust, and host-side capability enforcement remain release-gated.

This means: in MVP, the root binary is both daemon and GUI host in one process, exactly as `architecture.md` describes ("the GUI runs inside the daemon process on the main thread, while capture/store/sync run on background threads"). The "daemon" is a *role*, not yet a separate crate. The crate split into `vbuff-daemon`/`vbuff-ipc` is a refactor we perform once the single-process loop is proven, precisely so the CLI and sync have something to talk to.

---

## 1. Milestone-sequence overview

Phases follow the spine roadmap exactly: Phase 0 (foundations/scaffolding) -> MVP -> v1 -> v2. Bounded AI/integration contracts may land early to freeze privacy and compatibility boundaries, but enabling models, MCP, mobile, teams/collaboration, or off-LAN relay remains outside the current runtime milestones.

| Milestone | Name | Spine phase | Goal in one line | Primary crates | Single-process MVP? |
|---|---|---|---|---|---|
| **M0** | Scaffolding | Phase 0 | Workspace, CI skeleton, crate stubs compile on 3 OSes | all crates (empty) + root | yes (setup) |
| **M1** | Types + store core | Phase 0 | Data model, schema v1, WAL+SQLCipher, content-hash, golden vectors | `vbuff-types`, `vbuff-store` | yes |
| **M2** | Core engine on fakes | Phase 0 | Dedup, eviction, capture-gate, classify, search, all proptested headless | `vbuff-core` (+ `vbuff-store`, `vbuff-types`) | yes |
| **M3** | Platform traits + 1 backend + MockBackend | Phase 0 / MVP | The 4 traits, first OS backend, MockBackend, trait-conformance battery | `vbuff-platform` | yes |
| **M4** | First end-to-end loop (one OS) | MVP | copy -> store -> hotkey -> popup -> paste-back works on the first OS, encrypted | `vbuff-gui` + root + all above | yes |
| **M5** | Privacy, organization, settings, snippets/transforms (MVP set) | MVP | Fail-closed capture gate, pins/favorites, settings window, MVP snippets+transforms | `vbuff-core`, `vbuff-gui`, `vbuff-store` | yes |
| **M6** | Windows evidence beta | First-OS beta | Public app-pair matrix, 14-day dogfood, clean install, engineering and demand gates | `vbuff-platform`, `vbuff-store`, `vbuff-gui` | yes |
| **M7** | Conditional daemon + IPC extraction | Post-beta | Extract only when a second live client proves the process boundary is needed | `vbuff-daemon`, `vbuff-ipc` | no (gated) |
| **M8** | Acquisition + contextual recall | Post-beta | Safe competitor imports and a measured context-ranking experiment; optional CLI only after M7 | `vbuff-core`, `vbuff-store`, `vbuff-gui` | conditional |
| **M9** | Explicit LAN handoff | Post-beta experiment | One authenticated, selected, non-sensitive text transfer with TTL and receipt | `vbuff-sync` | no (gated) |
| **M10** | Windows release hardening | First native release | a11y/i18n, reliability, packaging and signing for the admitted Windows scope | all + packaging | conditional |
| **M11** | Gated expansion | Future | A second native backend before parity; ambient sync/AI/plugin work needs a separate proven gate | selected crates | no |

M0-M5 build the single-process Windows alpha and full-history path; M6 decides whether it deserves a beta. M7-M10 are independently activated post-beta work, not an automatic feature train. M11 is an explicit holding area for demand-gated expansion, not a promise of cross-platform parity or sync. Generic AI actions, OCR, mobile peers, team libraries, shared boards, ambient sync, MCP, and plugin execution are out of the active roadmap.

The expanded 600-idea backlog is reference material, not an implicit scope increase: engineering ideas 1-113 live in `architecture.md`, product/strategy ideas 114-197 live in `recommendation.md`, user-facing/operations ideas 198-300 live in `docs/ideas-top-300.md`, extended ideas 301-400 live in `docs/ideas-301-400.md`, review backlog items 401-500 live in `docs/ideas-401-500.md`, and evidence-backed ideas 501-600 live in `docs/ideas-501-600.md`. Twenty additional research candidates live separately in `docs/ideas-601-610.md` and `docs/ideas-611-620.md`; they do not extend the active objective. The 100-repository and primary-source evidence catalog lives in `docs/repositories-research-100.md`. The milestone gates below decide when an idea becomes planned work; repository popularity alone never promotes one.

| Range | Canonical backlog source |
|---|---|
| 1-113 | [architecture.md](architecture.md) |
| 114-197 | [recommendation.md](recommendation.md) |
| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) |
| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) |
| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) |
| 501-600 | [docs/ideas-501-600.md](docs/ideas-501-600.md) |

Post-600 research candidates: [601-610](docs/ideas-601-610.md) and [611-620](docs/ideas-611-620.md). They are intentionally outside the canonical range table.

The 2026-07-14 research pass found bundled SQLite 3.50.2 inside the old lockfile, within the WAL-reset bug range documented by SQLite. The baseline now uses `rusqlite 0.40.1` / bundled SQLite 3.53.2 with unneeded default features disabled and an integration test that denies affected engine versions. The deeper concurrent writer/checkpoint reproducer remains backlog item 582 rather than silently expanding M1.

Current baseline before the formal M7 crate extraction: the single-process root is divided into focused capture, history, guarded clipboard staging, command, diagnostics, single-instance, tray, autostart, configuration, maintenance, and event-loop modules. Serializable status contracts live in `vbuff-types`. Native egui is woken by hotkey, tray, and second-instance messages; a normal launch opens the popup and login registration uses `--background`. GUI navigation, projection, media, and Trust rendering are focused modules; UI preferences persist and reduced motion follows the OS while unset. The active clipboard adapter is generic `arboard` text-or-image polling, so source/concealed/generation/provenance and OS-history-exclusion capabilities are unavailable. Automatic injection is disabled; non-sensitive selection is copy-only and sensitive copy fails closed without history exclusion. Memory-only secret classes use a bounded, hard-expiring process lane and never enter SQLite/import. Store schema v7 provides SQLite/FTS/lifecycle machinery, but the live database is unencrypted and strict mode may block capture. A temporary migration safety copy is removed only after the upgraded or next-start store opens fully and passes `quick_check`; no durable rollback backup exists. IPC/plugin/sync/update and trust/recall/lifecycle types remain contracts unless a ledger identifies a connected runtime path; specifically, there is no daemon command listener, plugin process host or sandbox, browser/editor/mobile client, MCP/webhook endpoint, sync transport, or update installer. There is no web UI or WASM target. Preserve these boundaries until the corresponding milestone gates are actually passed.

### Batch execution overlay

The 600-item list is now executed in sequential batches of 50 without rewriting milestone scope. Each batch must classify every item as runtime, foundation, adapted, native-required, or rejected; complete three review passes; synchronize the four top-level documents; and pass formatting, strict clippy, tests, feature-variant checks, and whitespace review before commit/push.

| Batch | State | Evidence / next gate |
|---|---|---|
| 001-050 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-001-050.md) |
| 051-100 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-051-100.md); native APIs, SQLCipher, daemon dispatch, and sandboxed plugin-process execution remain milestone gates |
| 101-150 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-101-150.md); native OS conformance, release credentials, live updater/sync/plugin paths, and SQLCipher remain gates |
| 151-200 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-151-200.md); models, native integrations, daemon dispatch, SQLCipher, real compositor evidence, dogfood, and live sync remain gates |
| 201-250 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-201-250.md); plugin host, native caret/AT evidence, SQLCipher, OS-keystore recovery key, and native display validation remain gates |
| 251-300 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-251-300.md); continuous native auto-pause, live sync/transports/clients, SQLCipher, release credentials, and maintainer drills remain gates |
| 301-350 | Reviewed implementation/foundation complete | [Item ledger](docs/implementation-batch-301-350.md); three review iterations and local verification pass, while SQLCipher, native privacy/target proof, and plugin-host activation remain milestone gates |
| 351-600 | Queued in groups of 50 | Follow the shared range map and existing milestone ownership |

Batch completion does not override milestone acceptance. For example, item 163 can seal an embedding artifact and items 266-280 can define device UX while M9 remains open until pairing, authenticated transport, persistence, policy, and two-device replication work end to end. Likewise, SQLCipher remains an M1/M4 release blocker despite schema v7 lifecycle work. Batch 151-200 adds [registered decision gates](docs/decision-gates-151-200.md) and a [v1 data-contract freeze](docs/data-contract-v1.md); batch 201-250 adds [native/recovery gates](docs/decision-gates-201-250.md) and [data contract v2](docs/data-contract-v2.md); batch 251-300 adds [device/integration/operations gates](docs/decision-gates-251-300.md) plus the public operations records; batch 301-350 adds [trust/recall/lifecycle/native gates](docs/decision-gates-301-350.md) and [data contract v3](docs/data-contract-v3.md). Missing real-world evidence remains Unknown rather than passing by documentation.

From the first resident milestone onward, every milestone records the same SLO budget: zero silent loss among observed clipboard states, explicit accounting for every detectable sequence gap or intentional drop, search p99 at or below 16 ms, idle CPU at or below 0.5%, and login-ready at or below 500 ms. `Unknown` is a release blocker. A polling backend such as macOS `NSPasteboard.changeCount` cannot recover an intermediate payload after a detected gap and must report that limitation rather than claim universal zero-loss. Scope is similarly mechanical: more than nine current workspace crates, more than one added MVP milestone, or one milestone open beyond 42 days forces a cut-line review before new work starts.

---

## 2. Milestones in detail

For each milestone: **Goal**, **Phase**, **Crates/modules touched**, **Task checklist**, **Acceptance criteria**, **Target feature tiers on completion**, and **Pitfalls guarded**. Pitfall references use the catalog in `docs/mistakes-top-500.md` (numbered 1-N within its 18 categories) and the consolidated 25-row competitor-mistakes table in `architecture.md`; where a pitfall is the same item viewed twice, both are cited.

---

### M0 - Scaffolding

**Goal.** A Cargo workspace that compiles all (initially empty) crates on macOS, Windows, and Linux, with CI wired and the role-dispatch skeleton in place. No clipboard behavior yet.

**Phase.** Phase 0 (pre-MVP foundations).

**Crates/modules touched.** Workspace root `Cargo.toml`; stubs for `vbuff-types`, `vbuff-core`, `vbuff-store`, `vbuff-platform` (with `macos/`, `windows/`, `linux/{x11,wayland}/` module tree), `vbuff-gui`, `vbuff-daemon`, `vbuff-ipc`, `vbuff-plugin`, `vbuff-sync`, `vbuff-update`, `vbuff-cli`; root `src/main.rs`.

**Task checklist.**
- [ ] Create `[workspace]` `Cargo.toml` listing all nine member crates plus the root binary; pin a workspace-wide Rust toolchain (`rust-toolchain.toml`) and a single edition.
- [ ] Scaffold each crate with the exact canonical name and a `lib.rs`/`mod.rs` skeleton; encode the dependency direction (downward only) and add a CI check that `vbuff-core` does not depend on `vbuff-gui`, `vbuff-store`, `vbuff-platform` impls, or any OS crate.
- [ ] In `vbuff-platform`, create the `cfg`-gated module tree (`macos`, `windows`, `linux` -> `x11`/`wayland`) and a single `backends()` constructor signature returning a `Backends` struct (empty for now).
- [ ] In the root binary, implement the `LaunchOutcome` enum and `acquire_or_forward` skeleton (single-instance guard) returning `Daemon`/`Forwarded`; bind the IPC endpoint path per OS (`$XDG_RUNTIME_DIR`/`$TMPDIR`/named pipe) but with a no-op listener.
- [ ] Add a `MockBackend` placeholder in `vbuff-platform` behind a `cfg(test)`/feature flag so headless crates can build against it from day one.
- [ ] Stand up CI: per-OS build matrix (macOS, Windows, Ubuntu), `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`. Add the Linux X11 (`Xvfb`) and Wayland (headless `sway`) lanes as empty-but-present jobs so later milestones only have to fill them.
- [ ] Add `.gitignore`, license files (`LICENSE-APACHE`, `LICENSE-MIT`), and pin all platform-crate versions now so the maturity-assumption spike in M3 has a fixed target.

**Acceptance criteria.**
- `cargo build --workspace` and `cargo test --workspace` pass on all three OSes in CI.
- The dependency-direction lint fails the build if `vbuff-core` gains an illegal dependency.
- `vbuff` launched twice: the second invocation detects the bound endpoint and exits via `Forwarded` (even though it does nothing yet).

**Target feature tiers on completion.** None user-facing. Foundation for all tiers.

**Pitfalls guarded.**
- Business/maintenance pitfalls (`mistakes-top-500.md` section 18): set up the project so it is buildable and testable on all three OSes from commit one, the structural defense against the "abandonware / breaks on each OS" pattern that killed ClipX, 1Clipboard, ClipMenu, Flycut.
- Performance pitfall: a single pinned toolchain and pinned platform-crate versions guards against the silent-regression class (architecture.md crate-maturity caveat).

---

### M1 - Types and store core

**Goal.** The system of record: data model, schema v1 with migrations, WAL + SQLCipher open path, BLAKE3 content hashing with a golden-vector test, and the content-addressable blob store, all testable headless.

**Phase.** Phase 0.

**Crates/modules touched.** `vbuff-types` (`Clip`, `Flavor`, `Body`, `ContentKind`/`kind`, `ClipMeta`, `MimeType`, ids/ULID); `vbuff-store` (`schema`, `migrations`, `open` crypto path, `cas` blob store, `serde_clip`, `error`, `fts`).

**Task checklist.**
- [ ] Define `vbuff-types`: `Clip`, `Flavor { mime, bytes: Body::Inline | Body::Spilled(BlobRef) }`, `ClipMeta`, `ContentKind`, ULID-based `ClipId`. Depend only on `serde` (no rusqlite, no egui) so CLI/IPC can serialize later.
- [ ] Implement `content_hash(flavors)`: BLAKE3 over the MIME-sorted, length-prefixed, byte-for-byte flavor set (per architecture.md). Add a **golden-vector test** pinning the hash output so a future change that silently breaks dedup fails CI.
- [ ] Write schema v1 SQL exactly as architecture.md specifies: `item`, `flavor`, `source_app`, `tag`, `item_tag`, `collection`, `snippet`, `device`, `sync_log`, `meta_kv`, plus the FTS5 external-content `item_fts` + `item_text` + sync triggers, the partial unique index `idx_item_hash WHERE permanent=0 AND pinned=0`, and the facet indices.
- [ ] Implement the migration harness: `PRAGMA user_version`, forward-only ordered SQL steps each in one transaction, pre-migration online-backup, refuse-to-open on `user_version > MAX` (never auto-downgrade).
- [ ] Implement `open_encrypted`: SQLCipher key PRAGMA from a raw 256-bit key, `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `temp_store=MEMORY`, `mmap_size`, `busy_timeout`. Wrong key -> surface `SQLITE_NOTADB` as "locked", never wipe.
- [ ] Implement the CAS: payloads <=256 KiB inline (`storage=0`, zstd-`storage=2` when it helps); >256 KiB streamed to temp then `rename(2)` into `blobs/<2hex>/<full-blake3>`; `storage=1` with `blob_ref`. Reference-counted GC with a ~60s grace window.
- [ ] Implement the one-writer/pooled-reader split: a single `rusqlite::Connection` writer behind a channel/actor; `r2d2`+`r2d2_sqlite` read pool over WAL snapshots.
- [ ] Implement transactional-per-capture `upsert_capture` (item + flavors + FTS projection + eviction in one txn) and keyset (seek) pagination queries.
- [ ] Write the **whole-artifact canary test**: write `CANARY_SECRET`, close the DB, and scan raw `.db`/`-wal`/`-shm`/CAS/temp/log artifacts for zero hits. Wire it into the active Windows lane and reuse it for every promoted adapter.

**Acceptance criteria.**
- Schema v1 creates cleanly; migration harness applies a checked-in fixture from a synthetic v0 to v1 with zero data loss; refusing-newer-version path tested.
- Round-trip byte fidelity proptest: invalid UTF-8, NUL bytes, CRLF, trailing newlines, RTL, emoji, 4-byte codepoints survive store -> load unchanged.
- Canary-grep test green on macOS, Windows, Linux: no plaintext canary in any on-disk artifact.
- Golden content-hash vector matches; identical flavor sets produce identical hashes; any byte change changes the hash.
- WAL crash-recovery test: SIGKILL mid-transaction, reopen, last committed clip present, `integrity_check` clean.

**Target feature tiers on completion.** MVP storage substrate: encrypt-at-rest, transactional-per-capture durability, content-hash dedup key, out-of-row blob CAS, FTS5 schema present, and schema migration on upgrade.

**Pitfalls guarded.**
- History-wiped-on-update / no-schema-versioning / non-transactional migrations (`mistakes-top-500.md` 37, 50, 51, 75; architecture table #12): forward-only versioned migrations, pre-migration backup, refuse-don't-wipe.
- Unbounded DB growth, DB-never-shrinks, raw-image bloat, inline blobs (42, 43, 44, 60, 61; architecture table #14, #15): CAS + zstd + caps designed in.
- Plaintext/world-readable history; secret residue in freelist/WAL; encryption-silently-not-engaged (82, 54, 9-security; architecture table #20): SQLCipher + 0600 files + secure_delete + the canary-grep release blocker.
- DB-on-cloud-folder corruption (53; architecture table #18): per-machine `directories::ProjectDirs` path, warn-on-cloud-folder check.
- Dedup hashing the wrong representation (48): hash over the whole canonical multi-flavor set.
- DB corruption with no recovery (52, 73): WAL + `integrity_check` + quarantine-and-restart.

---

### M2 - Core engine on fakes

**Goal.** All pure policy logic (dedup, eviction/retention, the capture-decision gate, content classification, search ranking, snippet expansion logic) implemented in `vbuff-core` against trait-defined fakes (`FakeStore`, `FakeClipboard`), fully unit- and property-tested headless on any host.

**Phase.** Phase 0.

**Crates/modules touched.** `vbuff-core` (`classify`, `eviction`, `filter`, `hash`, plus new `gate`, `retention`, `search`, `snippet`, `dedup`); consumes `vbuff-types` and the `vbuff-store` traits/fakes.

**Task checklist.**
- [ ] Implement the **capture-decision gate** exactly as architecture.md's `evaluate(ctx, content) -> CaptureDecision` with the ordered `SkipReason` gauntlet: `MonitoringPaused`, `Incognito`, `ConcealedFlag`, `SecureInputActive`, `ExcludedApp`, `PrivateBrowsing`, `OverSizeLimit`, `WhitespaceOnly`, `PatternMatch`, `SourceUnknownFailClosed`, then `Capture{Normal|Sensitive}`. Cheap certain rejections first, content scanning last, every uncertainty -> `Skip`.
- [ ] Implement dedup + move-to-top: on capture, `find_by_hash`; exists -> bump `updated_at`/`use_count` and float; absent -> insert. Self-write suppression: a short-TTL fingerprint set keyed by content hash + owner.
- [ ] Implement eviction/retention: count cap (MVP), total-size cap, per-item cap, time expiry, sensitive-fast-path expiry, unlimited mode; all skip pinned/favorite/permanent.
- [ ] Implement the content classifier (`classify`) producing `ContentKind` (Text/RichText/Html/Image/Files/Color/Url/Code/Mixed) once at capture; build the `item_text` searchable projection (e.g. "PNG 1920x1080 from Safari").
- [ ] Implement built-in secret detectors (Luhn cards, JWTs, PEM keys, AWS keys) -> `Sensitivity::Sensitive`; anchored, size-bounded `regex` with short-circuit.
- [ ] Implement substring search (MVP tier) and the FTS-narrow-then-fuzzy-rank pipeline (the `nucleo` ranker, used for the v1 fuzzy tier); live-highlight offsets.
- [ ] Implement MVP snippet expansion logic: token substitution (`{date}`, `{time}`, `{clipboard}`, `{cursor}`) as pure functions.
- [ ] Build `FakeStore` and `FakeClipboard` and the property test suites: pinned-never-evicted under any cap; byte fidelity round-trip; identical content never two rows; gate invariant (paused/incognito/concealed/secure-input/source-unknown-required ALWAYS yield `Skip` - a `Capture` here is a release blocker).

**Acceptance criteria.**
- The gate is table-driven with one test per `SkipReason` plus `Capture+Normal`/`Capture+Sensitive`; the fail-closed proptest passes with no `Capture` escaping for any kill-switch input.
- Secret-detector corpus test tracks precision/recall and fails CI on regression (valid cards/JWTs/PEM/AWS -> Sensitive; UUIDs/hashes/prose -> Normal).
- Eviction proptest: under randomized cap pressure, pinned/favorite/permanent items survive every cap type.
- All M2 logic runs with zero OS dependencies in the headless CI lane.

**Target feature tiers on completion.** MVP: dedup-by-hash + move-to-top, count cap with pin exemption, incognito/pause, concealed-flag policy, app blacklist policy, whitespace skip, content-type detection/labeling (the `[MVP]` extras items 7, 8), MVP snippets (date/time placeholders), and MVP transform scaffolding.

**Pitfalls guarded.**
- Ignoring concealed/transient hints, capturing from every source, no default deny-list (`mistakes-top-500.md` 7, 8, 10, 69, 70; architecture table #7, #8): the gate honors hints and applies the default deny-list before any byte persists.
- Echo loop / infinite image re-add / no suppression window (18, 19, 35, 62; architecture table #9): fingerprint + short suppression window + content-hash dedup.
- No deduplication / duplicate clutter / dedup wrong representation (25, 47, 48; architecture table #10 multi-flavor): hash the full canonical set, bump-don't-insert.
- Pinned evicted by trimming / counted against cap (40, 41; architecture table #13): pins exempt from every eviction query, separate class.
- No retention / no sensitive auto-expiry (56): dual-timer retention with a shorter sensitive expiry.
- Reading only text/plain (13, 67; architecture table #10): classifier and projection operate over the whole flavor set, never discarding representations.

---

### M3 - Platform abstraction: traits, first OS backend, MockBackend (dedicated early milestone)

**Goal.** Lock the `vbuff-platform` trait surface, implement the **first** concrete OS backend end-to-end, and ship a `MockBackend` plus a shared trait-conformance test battery. This is the dedicated platform-abstraction milestone the rest of the build leans on.

**Phase.** Phase 0 / MVP boundary (the traits are Phase 0; the first real backend begins MVP).

**Crates/modules touched.** `vbuff-platform` (`lib.rs` trait defs; the active per-OS impl; `mock`); the `cfg`/runtime `backends()` selector; `error`.

**Decision.** Prove the product on a deliberately narrow **Windows 11 x86-64** interactive-session scope first. Windows offers event-driven sequence evidence, native format enumeration, clipboard-owner attribution, distinct monitor/history/cloud markers, global hotkeys, and target-reconfirmable delivery in one bounded vertical. This maximizes product evidence rather than platform-risk retirement. Wayland still gets an early reality spike, but no compositor blocks the first beta or inherits parity from shared capability models. The MockBackend and conformance battery remain OS-agnostic.

**Task checklist.**
- [ ] Finalize the four traits verbatim from architecture.md: `ClipboardBackend` (`run(sink, ctl)` / `read_all` / `write` / `is_concealed` / `capabilities`), `HotkeyBackend` (`register`/`unregister`/`events`/`is_available`), `PasteBackend` (`capture_focus` -> `FocusToken`, `paste_into`, `type_text`, `capabilities`), `TrayBackend` (`install`/`update`/`events`). Add the adjacent `SecretStoreBackend` and `AutostartBackend` traits.
- [ ] Define the shared event/value types: `CaptureEvent`, `Flavor`/`FormatKey`, `Sensitivity`, `SourceApp`, `Control { Pause|Resume|SnapshotNow|Shutdown }`, `PasteCaps`, `Conflict`, `Capabilities`.
- [ ] Keep the **format-mapping table** and checked-in `format-fidelity-v1` corpus as the shared pre-backend oracle (UTI / CF_* / MIME <-> `FormatKey`), preserving unknown custom identifiers and byte-identical round trips.
- [ ] Introduce the ordered `ClipboardSnapshot -> ClipboardItem[] -> Flavor[]` contract with native format IDs, realization state, generation evidence, and one canonical encoding used by hashing, persistence, export, and fidelity comparison.
- [ ] Implement the selected Windows `ClipboardBackend`: `WM_CLIPBOARDUPDATE`, `GetClipboardSequenceNumber`, owner evidence, bounded native-format enumeration, delayed-render accounting, exact bytes, and the distinct monitor/history/cloud policy markers.
- [ ] Implement Windows hotkey and delivery evidence: native registration, focus captured before opening, immediate target reconfirmation, clipboard write, `SendInput` only for a matching non-elevated target, and copy-only fallback for every unproven case. Keep tray, autostart, and key-provider decisions behind narrow adapters.
- [ ] Limit the alpha matrix to declared Windows 11 sessions, applications, and formats; elevated targets, RDP, images/files/custom formats, and sensitive paste remain unsupported until their own evidence rows pass.
- [ ] Implement `MockBackend`: a scriptable `ClipboardBackend`/`PasteBackend`/`HotkeyBackend`/`TrayBackend` emitting scripted `CaptureEvent`s and recording writes, with zero OS dependency, for driving daemon policy in CI.
- [ ] Write the **trait-conformance battery**: a parameterized `#[test]` suite runnable against any backend impl (Mock now, real backends as they land), asserting read-all returns every offered flavor, self-writes are suppressed, capabilities are reported honestly, and `capture_focus` precedes `write` on paste.
- [ ] Execute the documented **build-versus-buy ladder**: verify Win32 bindings, tray integration, Credential Manager/DPAPI plus SQLCipher, and the native crates against pinned versions; use/wrap/fork/degrade only at the registered trigger. Run `scripts/wayland-reality-check.sh` separately to maintain an honest future capability matrix.

**Acceptance criteria.**
- The conformance battery passes against `MockBackend` and the selected Windows backend; 10,000 native edges are captured exactly once or every sequence gap is explicit.
- At least `CF_UNICODETEXT` and CF_HTML pass 1,000 canonical round trips per declared app pair; unsupported formats remain identifiable and never silently flatten into `Other`.
- Target tests produce zero wrong-window injections, and plaintext canaries appear in none of DB/WAL/SHM/CAS/temp/log artifacts. First-session idle CPU remains within budget over a multi-hour run.
- The crate-maturity spike report is committed; any swapped crate is reflected in pinned versions.

**Target feature tiers on completion.** Evidence-backed Windows alpha: honest native monitoring, declared format topology, history/monitor policy handling, native hotkey, target-confirmed paste or copy-only fallback, tray UI, encrypted key access, and autostart on one documented session class. This is not a cross-platform claim.

**Pitfalls guarded.**
- Fixed-interval polling misses copies / re-reading content every tick / idle CPU burn (`mistakes-top-500.md` 1, 2, 32; architecture table #1, #2): poll only the integer changeCount, read content once per edge, adaptive backoff.
- Missing rapid successive copies (3; architecture table #3): capture on the change edge, enqueue, persist off-thread.
- Capture-races-with-owner / read-before-ready / holding clipboard open (26, 27): shortest-possible open, retry transient lock failures, release immediately.
- Capture monitor crash/hang / broken viewer chain (22, 23; architecture table #21, #6): modern listener APIs; the watchdog lands fully in M7 but the re-subscribe hook is designed here.
- Blank/empty entries / delayed-render not realized (15, 16, 66; architecture table #11): validate at least one representation has real bytes before commit.
- macOS Accessibility fragile / silently dropped (pain-points): copy-only degradation with a guiding banner, never a silent no-op.

---

### M4 - First end-to-end loop on one OS/session scope

**Goal.** The product's core loop - copy -> store -> hotkey -> popup -> paste-back - works end-to-end on the first explicitly supported OS/session scope, encrypted at rest, fully keyboard-driven, in a single process.

**Phase.** MVP.

**Crates/modules touched.** `vbuff-gui` (eframe app: popup + settings viewports, virtualized list, filter/highlight, keyboard nav, theming); root `src/main.rs` (wire watcher thread -> store actor -> core -> GUI via `Arc` + channels; single-instance summon). Consumes all prior crates.

**Task checklist.**
- [ ] Build the eframe app with two viewports: the popup and (stub) settings. GUI on the main thread; capture on the dedicated watcher thread; store actor owns the writer.
- [ ] Implement the popup: opens near the cursor, clamps to the work area; `ScrollArea::show_rows` virtualized list reading the store directly via the read pool; type-to-filter (substring tier) with live highlight; type icons, image thumbnails, pinned/favorite badges; empty/no-results state; dismiss on focus-loss/Escape.
- [ ] Implement full keyboard navigation: arrows, Home/End, Enter to paste-back, digit quick-pick, Escape to dismiss; the entire open -> filter -> navigate -> paste flow without the mouse.
- [ ] Wire paste-back through `PasteBackend`: snapshot focus FIRST, optionally snapshot current clipboard, write chosen flavors, dismiss popup, restore focus, inject Cmd+V; plain-vs-keep-formatting choice; copy-only fallback when Accessibility is denied.
- [ ] Wire the global hotkey to capture focus then summon/toggle the popup; cold-start so the hotkey and tray are live within a few hundred ms of process start.
- [ ] Connect the capture path: watcher emits `CaptureEvent` -> core gate -> store upsert (transactional) -> popup sees new items via the store.
- [ ] Dark/light themes; DPI scaling; accesskit tree for the list.
- [ ] Pull packaging left: install the first Windows artifact on a clean environment and run doctor/verification before public beta; source-checkout-only success does not pass M4.

**Acceptance criteria.**
- On the selected first scope: copy in app A, summon the popup through the documented capability path, filter, press Enter, and content lands in app A or the UI explicitly selects copy-only behavior.
- The whole flow is mouse-free.
- Cold-start: hotkey live < a few hundred ms after launch.
- Search-as-you-type stays under ~16 ms/frame at 50,000+ seeded items (virtualized, keyset-paged, no `SELECT *`).
- Restored/self-written clips do not create new rows (suppression confirmed in the live loop).

**Target feature tiers on completion.** MVP core: the headline copy->store->hotkey->popup->paste-back loop; substring search-as-you-type with highlight; recency default; keyboard nav; paste-back (plain/rich); tray item; popup near cursor; thumbnails; pin/favorite badges; restore-session-on-launch.

**Pitfalls guarded.**
- Core paste silently fails / pastes wrong window / leaks modifiers / popup steals focus (`mistakes-top-500.md` section 4; architecture table #17): capture_focus first, confirm target frontmost, clear modifiers, surface failures.
- Loading the entire history into the UI / slow search at scale / performance collapse (63, 64, 16; architecture table #16): virtualized list + keyset pagination + FTS-narrowed search.
- Popup focus theft / wrong formatting on paste (pain-points macOS): Spotlight-style focus handling and explicit plain-vs-rich.
- Cold-start slowness that "feels like a crash" (pain-points): defer GUI construction, live hotkey first.

---

### M5 - Privacy gate hardening, organization, settings, MVP snippets + transforms

**Goal.** Complete the MVP feature set beyond the bare loop: the full fail-closed privacy surface in the live app, organization (pins/favorites/permanent/type-filter), the real settings window with hotkey conflict detection and onboarding, and the MVP snippet and transformation sets.

**Phase.** MVP.

**Crates/modules touched.** `vbuff-core` (gate wiring to live config, transforms), `vbuff-gui` (settings viewport, organization UI, snippet editor, quick-action palette), `vbuff-store` (snippet/collection tables in use, pin/permanent flags), `vbuff-platform` (secure-input detection, autostart, screen-lock signals).

**Order gate.** Privacy is the non-cuttable first half: capture/AI eligibility, visible health, deny rules, secure deletion, and retention must pass before snippets, recipes, or transform breadth can consume schedule. Convenience is the explicit cuttable tail.

**Task checklist.**
- [ ] Wire the live capture gate to runtime config read via `Arc<RwLock<…>>`: pause/resume, incognito, per-app exclusion (with a default deny-list of known password managers), whitespace skip, secure-input skip (macOS `IsSecureEventInputEnabled`), concealed-flag honoring already in core.
- [ ] Keep `ai_allowed` affirmative and fail closed across capture, embedding, search, explanation/caption/PII backends, sync artifacts, and later external endpoints; legacy/unknown eligibility is denied.
- [ ] Organization: pin-to-top, star favorite, promote-to-permanent (out of the auto-prune pool), persistent-vs-ephemeral, type filter chips, pinned/favorites filter, pin-protected-from-clear; manual reorder via fractional `sort_index`.
- [ ] Clear-all (pin-protected) + delete-item; secure-delete path for sensitive items; "wipe all incl. pinned" panic option.
- [ ] Settings window: launch-at-login (`AutostartBackend`), start-minimized, retention limits (count cap, per-item cap), the hotkey editor with bind-time conflict detection (`HotkeyBackend::is_available`), storage-location display with cloud-folder warning, onboarding/permissions flow (deep-link to macOS Accessibility), and a visible capture-health/persistence indicator.
- [ ] MVP snippets: saved snippets, abbreviation expansion, insert-by-hotkey, search-in-popup, folders, built-in editor, date/time placeholders, promote-clip-to-snippet.
- [ ] MVP transformations: change case, trim whitespace, strip formatting, paste-as-plain one-shot, literal find&replace, quick-action palette - all as `vbuff-core` pure transforms applied at paste time (never to stored canonical bytes).
- [ ] Auto-clear-on-timer and the sensitive-fast-path expiry surfaced in settings; idle auto-lock plumbing (full lock UX is v1).

**Acceptance criteria.**
- Copying from a default-excluded password manager never persists (verified by a test asserting nothing reaches `vbuff-store`).
- Pinned/permanent items survive clear-all and fill-past-cap; manual order survives restart.
- Binding a taken hotkey is refused with a visible conflict message.
- An abbreviation expands; a clip promotes to a snippet; a transform applies on paste without mutating the stored clip.
- The capture-health indicator shows "capturing / paused" so silent loss is visible.
- Before M6 beta, complete a 14-day Windows dogfood window as the only clipboard manager with zero silent observed-state loss and zero wrong-target injections; missing evidence blocks beta.

**Target feature tiers on completion.** MVP security/privacy set (encrypt-at-rest, skip password fields, honor concealed markers, per-app exclusion, incognito, auto-clear-on-timer, wipe-on-demand, local-by-default, zero telemetry); MVP organization, settings, snippets, and transforms. Extras items tagged `[MVP]`: 34 (strip formatting), 49 (set phrases/snippet bank), 57 (app exclusion), 59 (auto-search quick paste), 69 (paste-as), 79 (per-app exclusion rules), 80 (type-ahead picker), 106 (paste as plain text), 122 (pinned persisting as snippet bank).

**Pitfalls guarded.**
- Secrets into history / no default exclusion / OTP capture / unverifiable trust (`mistakes-top-500.md` section 10; pain-points security): default deny-list + secure-field skip + secret detectors + masked sensitive + zero telemetry.
- Settings silently reset/corrupted (74): versioned, atomically-written config with last-good restore.
- Hotkey conflicts / silently stops working (section 12; pain-points Windows "failed to set hotkey"): bind-time conflict probe, refuse-and-explain.
- Wrong formatting on paste / paste-as-plain collisions (pain-points macoS #109/#1232): explicit plain-vs-rich and conflict-free transform shortcuts.
- Treating capture as best-effort with no observability (36; architecture table #21): visible capture-health state.
- Residual data after clear-all (55): clear removes DB, sidecars, blobs, thumbs, index, then VACUUM.

---

### M6 - Windows evidence beta

**Goal.** Turn the admitted Windows 11 alpha into a measured beta without widening platform scope.

**Phase.** First-OS beta decision.

**Crates/modules touched.** `vbuff-platform`, `vbuff-store`, `vbuff-core`, `vbuff-gui`, the root runtime, Windows packaging, and the public fidelity evidence generator.

**Task checklist.**
- [ ] Publish a versioned source-app -> target-app matrix for the declared browser, IDE, terminal, and Office routes. Every row links to corpus input, canonical comparison, capability state, and failure evidence.
- [ ] Prove the DB-backed `HistoryQuery -> HistoryPage` path at 100,000 rows, including retrieval beyond the first 1,000, stale-query cancellation, volatile-lane merge, and selected-item-only hydration.
- [ ] Run 10,000 clipboard edges, 1,000 round trips per supported format, controlled crash/restart, target-change, clipboard-contention, autostart, and whole-artifact plaintext-canary drills on real Windows 11 hosts.
- [ ] Install a signed candidate on a clean user profile; verify key creation/recovery, update-safe migration, launch-at-login, hotkey conflict reporting, tray state, keyboard-only use, text scaling, and NVDA navigation.
- [ ] Dogfood for 14 days as the sole clipboard manager. Record observed sequence gaps, degraded routes, wrong-target prevention, idle CPU, memory growth, startup latency, and crash recovery instead of replacing missing evidence with a capability claim.
- [ ] Recruit 20 target users and measure activation, retained weekly use, and willingness to pay using the fixed gate below.

**Acceptance criteria.**
- Zero silent loss of an observed Windows clipboard state; every sequence discontinuity is visible and never described as recovered.
- Zero wrong-target injections and zero plaintext canary hits in DB/WAL/SHM/CAS/temp/export/log/crash artifacts.
- Supported app-pair rows preserve canonical bytes; degraded and unsupported rows never silently flatten or inject.
- Full-history retrieval meets p95 <= 50 ms for first results and warm interactive p99 <= 16 ms.
- Of 20 target users, at least 12 activate without documentation, 8 use vbuff four days per week after 30 days, and 5 are willing to pay at least USD 25.

**Target feature tier on completion.** One evidence-backed Windows beta. It is not a cross-platform, sync, MCP, plugin, or destination-only secrecy claim.

**>>> FIRST-OS BETA GATE here (see section 5).**

**M6 -> post-beta contract gate.** Freeze the on-disk schema, content-hash vector, native format keys, and IPC serde representation through [data-contract-v1.md](docs/data-contract-v1.md) and executable goldens before any second client or backend consumes them. A later break requires a new version, migration/negotiation behavior, and old-reader tests.

---

### M7 - Daemon and IPC extraction

**Goal.** If a second live client is approved after M6, refactor the proven single-process wiring into the canonical `vbuff-daemon` and least-privilege `vbuff-ipc` boundary. The split is not a quality milestone by itself.

**Phase.** Conditional post-beta architecture work. Keep the root process intact unless a real CLI, integration, or separately supervised host owns the client side.

**Crates/modules touched.** New `vbuff-daemon` (owns background threads/runtime, wires watcher<->store<->core<->IPC); new `vbuff-ipc` (UDS/named-pipe framing, `ClientIntent`/`Response` serde); root `src/main.rs` (role dispatch now delegates to `vbuff-daemon`); `vbuff-platform` (watchdog re-subscribe hook).

**Bootstrap already landed in the root app.** `ClientIntent::{ShowPopup, Ping}` and `ServerResponse` are serializable in `vbuff-types`; the root owns bounded framing, Unix-socket or authenticated Windows-loopback bind-or-forward, liveness probing, stale-endpoint recovery, and capture heartbeat/stalled visibility. This is an intentionally narrow precursor, not completion of M7: native hook re-subscribe/auto-restart, the Windows named pipe, process-level handoff tests, daemon extraction, and the full control protocol remain below.

**Task checklist.**
- [ ] Extract the watcher/store/core wiring from the root binary into `vbuff-daemon`; the daemon owns the tokio runtime for IPC/timers and the dedicated (non-tokio) capture thread.
- [ ] Implement the supervised capture component: heartbeat/watchdog that detects a stalled listener (via a known self-write probe) and re-registers OS hooks; auto-restart on backend crash; surface "capturing / paused / unsupported / listener restarting" health.
- [ ] Implement `vbuff-ipc`: framed serde protocol over Unix domain socket (`$XDG_RUNTIME_DIR`/`$TMPDIR`) and Windows named pipe (`\\.\pipe\vbuff-<user>`); the `ClientIntent` verbs (`ShowPopup`, `AddClip`, `Paste`, `Watch`, control verbs) and `Response`s; client + server.
- [ ] Harden the single-instance guard handoff: bind-or-forward, stale-socket unlink-and-retry-once, liveness probe.
- [ ] Move retention timers (count/size/time/sensitive) onto the daemon runtime.
- [ ] GUI continues in-process with the daemon (shared `Arc` + store actor channel); only external clients pay IPC cost.

**Acceptance criteria.**
- The admitted Windows beta matrix still passes after the refactor; future native adapters must pass independently when promoted.
- IPC contract test: serialize/deserialize every `ClientIntent`/`Response`; two-process single-instance handoff spawns and forwards correctly.
- Watchdog test: simulate a stalled listener, assert re-subscribe and a visible health change.
- Stale-socket-after-crash: unlink and rebind once, verified.

**Target feature tiers on completion.** v1 platform/robustness: supervised always-on capture, capture-health observability, single-instance handoff hardening, and the daemon control socket the CLI needs.

**Pitfalls guarded.**
- Capture monitor crash/hang takes down recording silently / capture stops when another process owns clipboard (`mistakes-top-500.md` 22, 36, 77; architecture table #21): watchdog + re-subscribe + visible health.
- Two instances launched / stale socket (architecture failure-modes table): bind-fail forward, unlink-rebind-once.

---

### M8 - Acquisition and contextual recall

**Goal.** After the Windows beta earns continued investment, reduce switching cost and prove that privacy-safe context improves retrieval. A CLI is optional and requires the live M7 boundary; scripting breadth is not part of this milestone.

**Phase.** Conditional post-beta product depth.

**Crates/modules touched.** `vbuff-core` (deterministic facets/ranking), `vbuff-store` (source/session metadata and import staging), `vbuff-gui` (context strip and import review), import adapters, and optionally `vbuff-cli`/`vbuff-ipc` after M7.

**Task checklist.**
- [ ] Add dry-run importers for official exports or safe snapshots from Maccy, CopyQ, Ditto, and PasteBar. Never attach to a rival's live database.
- [ ] Preserve ordered item/flavor topology, pins, timestamps, source metadata, and policy state where representable; emit a machine-readable loss manifest for everything else.
- [ ] Add encrypted source application, time, session, and neighboring-copy facets behind one deterministic `RecallQuery` boundary.
- [ ] Build 200 labeled retrievals and compare the current text-only baseline with contextual ranking. Record accepted top-three choices; do not tune on the evaluation set.
- [ ] Add a compact context strip and “surrounding copies” action without changing stable row height or hiding capability uncertainty.
- [ ] Only if users demonstrate an external-workflow need after M7, add a least-privilege CLI for bounded list/search/copy commands. No shell execution, plugin host, MCP server, or broad history grant enters this milestone.

**Acceptance criteria.**
- Each importer supports preview, cancellation, deterministic rerun, duplicate accounting, and rollback; sensitive material is excluded from unencrypted exports by default.
- Contextual ranking improves accepted top-three retrieval by at least 20% across the labeled set without violating the full-history latency budget. Otherwise remove destination learning and keep deterministic facets only.
- The popup remains keyboard-complete, screen-reader navigable, and honest when source/session evidence is unavailable.
- Any optional CLI authenticates to M7, enforces the same typed policy as the GUI, and cannot request secrets by default.

**Target feature tier on completion.** Measured recall differentiation and a low-risk migration path, with no implied scripting, plugin, MCP, or cross-platform commitment.

---

### M9 - Explicit LAN handoff

**Goal.** Send one explicitly selected, TTL-bound clip to one authenticated paired device over the LAN. This milestone proves identity, replay protection, policy, expiry, durable one-shot semantics, and receipts before any ambient history replication exists.

**Phase.** Post-beta experiment. Activate only after the first native beta passes engineering and demand gates; otherwise keep `vbuff-sync` frozen.

**Crates/modules touched.** `vbuff-sync` (device identity, authenticated pairing transcript, one-shot envelope, replay/expiry state); `vbuff-store` (paired devices and durable nonce/receipt state); runtime transport; `vbuff-gui` (pair and Send flow).

**Task checklist.**
- [ ] Bind Ed25519 signing and X25519 agreement keys in one `DeviceIdentity`; sign membership changes and a versioned pairing transcript covering identities, ciphersuite, capabilities, nonces, and the membership head.
- [ ] Pair through explicit SAS/QR confirmation using a proven PAKE/Noise implementation; store authenticated peer identity and revocation state.
- [ ] Send only one explicitly selected, non-sensitive, `sync_eligible` text clip to one recipient, with a 64 KiB cap and five-minute TTL.
- [ ] Persist nonce/replay/one-shot state across crashes; acknowledge only after the recipient commits the clip durably. Keep the recipient clipboard untouched until the user selects the item.
- [ ] Exclude discovery breadth, relay, CRDT, Merkle reconciliation, images/files, background monitoring, and every auto-sync-on-copy path from this milestone.

**Acceptance criteria.**
- Two paired devices complete 1,000 explicit sends with LAN p95 below two seconds and no wrong-recipient, replayed, expired, duplicate, sensitive, or local-only transfer.
- MITM, unknown-key-share, downgrade, key substitution, forged add/revoke, and replay tests all reject; packet and log scans reveal no payload plaintext.
- Crash/restart preserves one-shot consumption and receipt semantics. Failure to prove this falls back to signed encrypted transfer bundles, not ambient sync.

**Target feature tiers on completion.** One-target E2EE handoff only: authenticated pairing, explicit recipient, TTL, replay protection, policy exclusion, durable one-shot handling, and a signed receipt. Ambient replication remains a later independently gated milestone.

**Pitfalls guarded.**
- Insecure/paywalled/dead-backend transfer (`mistakes-top-500.md` section 9; architecture table #19): E2E encrypted, direct LAN first, no vendor dependency.
- DB-on-cloud-folder corruption (53; architecture table #18): record-level sync, never the raw SQLite file.
- Sensitive copies synced by hidden default (pain-points security): sensitive/local-only excluded by construction; sync is explicit.
- Fragile delivery that silently duplicates or expires: durable nonce state, explicit TTL, one-shot commit, and visible receipts.

---

### M10 - Windows release hardening, i18n/a11y, release engineering

**Goal.** Close the first native release: content shaping, accessibility and internationalization, performance verification, crash safety, and Windows packaging/signing for the session and app matrix admitted by M6.

**Phase.** Conditional first native release; activate only after the M6 engineering and demand gates.

**Crates/modules touched.** `vbuff-gui` (cosmic-text galley for clip content, a11y roles, i18n, density/theme polish); `vbuff-store` (crash-recovery, integrity, backup); packaging configs across all crates; CI release lanes.

**Task checklist.**
- [ ] Integrate the `cosmic-text`-backed galley layer for clip *content* text (CJK/Indic/Arabic/emoji/RTL); keep egui chrome on egui. RTL mirroring + BiDi.
- [ ] Accessibility: `accesskit` roles for popup and settings, screen-reader live announcements, focus traps, high-contrast, reduced-motion, keyboard cheat-sheet, onboarding tour.
- [ ] i18n: localization + locale-aware date/time formatting (UI layer only; storage stays UTC epoch-millis), complex-script shaping verified.
- [ ] UI polish: follow-OS-theme, per-type row styling, full preview pane, density toggle, font/size choice, resizable + remembered window, multi-monitor placement, HiDPI crispness, search-result highlighting.
- [ ] Robustness: WAL crash recovery in production, `integrity_check` at startup with quarantine-and-restart, online-backup before migrations, disk-full handling (abort capture txn, pause, notify), CAS orphan GC, owner-contention backoff.
- [ ] Master password (v1): wrap the root DEK with `Argon2id`; quick PIN (rate-limited); idle auto-lock + OS screen-lock signal handling; locked collections; confirm-before-clear; clear-on-exit; masked sensitive clips; private-browsing exclusion where the OS allows.
- [ ] Performance verification: criterion benchmarks for sub-frame search at 100k items, insert/evict throughput, cold-start; multi-day idle-CPU-near-0% soak; memory bounded (no linear leak).
- [ ] **Windows release/packaging/signing** (see section 4): signed installer, clean-profile install/uninstall/upgrade, autostart registration, Credential Manager/DPAPI lifecycle, and an explicit supported-session matrix. Future OS packaging is activated only with its independently proven adapter.

**Acceptance criteria.**
- RTL/CJK/emoji clip content renders correctly via cosmic-text; a screen reader navigates the popup.
- Sub-frame search holds at 100k items; multi-day soak shows no memory leak and near-0% idle CPU.
- Master-password wrap/unwrap works; wrong password fails without wiping; password change preserves DB readability.
- The signed Windows artifact installs, upgrades, recovers, and uninstalls cleanly on a fresh profile; no SmartScreen block remains after the chosen signing reputation gate.

**Target feature tier on completion.** A hardened Windows release with measured UI, security, reliability, accessibility, and packaging evidence. It does not imply another native backend.

**Pitfalls guarded.**
- egui weak BiDi/complex-script blocks RTL/CJK/Indic users (architecture risk table): cosmic-text content layer.
- Performance: linear memory leak / clearing doesn't free / idle CPU spin / whole-history render (`mistakes-top-500.md` section 16; pain-points performance): bounded working set, virtualization, event-driven idle.
- Accessibility/i18n gaps (section 17): accesskit roles, localization, complex-script shaping.
- Blocked launch via signing/notarization gaps (pain-points business; section 18): proper notarization/signing in the pipeline.
- DB corruption / power loss / non-atomic writes (52, 73, 75; architecture failure modes): WAL + integrity_check + atomic writes + backups.

**>>> FIRST NATIVE RELEASE GATE here (see section 5).**

---

### M11 - Gated expansion

**Goal.** Hold future work behind evidence instead of turning the backlog into a release promise.

**Phase.** Future; no item activates automatically when M10 completes.

**Promotion rules.**
- [ ] Choose a second native OS from paid-beta demand. Implement its real adapter and rerun the same edge, fidelity, privacy, target, residue, accessibility, soak, packaging, and app-pair gates before making any parity claim.
- [ ] Promote ambient sync only after M9 succeeds and users still demand history replication. Write a separate threat model and gates for membership, revocation, convergence, offline recovery, relay metadata, and wipe receipts.
- [ ] Promote MCP, plugins, OCR, AI actions, mobile peers, or team collaboration only with a narrower user job, a least-privilege boundary, misuse tests, and an owner. Existing contract crates do not count as activation.
- [ ] Keep generic transforms, shared boards, marketplaces, and feature parity below fidelity-corpus and contextual-recall work unless measured retention changes the order.

**Acceptance criteria.** Each promoted item gets a dated decision record, an explicit owner, a smallest vertical slice, a kill metric, and independent release evidence. Unpromoted work remains foundation or backlog and is described that way in every top-level document.

**Target feature tier on completion.** None by default. M11 is a governance gate, not a bundled v2 scope.

---

## 3. Testing and CI strategy

The testing posture is inherited from architecture.md's consolidated testing strategy and made concrete per milestone above. The crown jewel is `vbuff-core`: because it depends only on traits, almost all behavior is tested headless on any host.

### Layered test types
- **Unit tests** in every crate for pure functions (gate decisions, transforms, format mapping, classify, fractional indexing).
- **Property tests** (`proptest`) for the invariants that must never break: byte-for-byte fidelity (invalid UTF-8, NUL, CRLF, trailing newlines, RTL, emoji, 4-byte codepoints survive round-trip); pinned/favorite/permanent never evicted under any cap; identical content never two rows; the fail-closed gate invariant (paused/incognito/concealed/secure-input/source-unknown-required always `Skip`).
- **Corpus tests** for secret detectors with tracked precision/recall, failing CI on regression.
- **Golden-vector tests** for the BLAKE3 content hash (changing it silently breaks dedup -> release blocker).
- **Crypto tests:** seal/open round-trip; one-byte tamper of ciphertext/nonce/tag -> AEAD error; wrong key -> failure not garbage; master-password wrap/unwrap; whole-artifact canary scanning on the active Windows lane and every independently promoted adapter.
- **Store tests:** migrations forward-apply on checked-in fixture DBs from each prior schema version; WAL crash-recovery by SIGKILL mid-transaction; disk-full via a small loopback/quota FS; FTS5 latency benchmark at 50k+ rows (< 8 ms for the SQL+map step); FTS correctness (diacritic folding, case-insensitivity, prefix matching, CJK tokenization, highlight offset alignment).
- **Concurrency/contention tests:** writer thread + N reader threads paging/searching; a stress harness writing the clipboard from N threads/processes; assert no `SQLITE_BUSY` escapes, no deadlock, bounded retries, no duplicate rows beyond dedup, snapshot consistency under WAL.
- **IPC contract tests** (from M7): serialize/deserialize every `ClientIntent`/`Response`; two-process single-instance handoff.
- **GUI tests:** filter/highlight/selection extracted into pure functions tested headless; egui rendering smoke-tested via `egui_kittest`; permission degradation injects `PasteCapability::ClipboardOnly` and asserts copy-only fallback.
- **Benchmarks** (`criterion`): type-to-filter latency at 100k items, insert/evict throughput, cold-start.

### Trait mocking
The `MockBackend` (built in M3) implements all four backend traits with scripted `CaptureEvent`s and recorded writes, with zero OS dependency. The **trait-conformance battery** is a parameterized `#[test]` suite runnable against any `ClipboardBackend`/`PasteBackend`/`HotkeyBackend`/`TrayBackend` impl; Mock runs it everywhere, each real backend runs it in its OS lane. This lets daemon policy be driven deterministically in CI and lets a new backend prove conformance before it ships.

### Native evidence matrix
- **Windows (active release lane):** full suite, Windows backend conformance, real-host edge/fidelity/target tests, whole-artifact canary scan, NVDA evidence, clean install, and signing checks.
- **Headless host (active):** all `vbuff-core`/`vbuff-store` tests against fakes plus `MockBackend`; this is the fastest broad signal but never substitutes for native evidence.
- **macOS (future adapter template):** full suite, backend conformance, Keychain/Accessibility/autostart evidence, canary scan, VoiceOver, notarization, and app-pair matrix after M11 promotes it.
- **Ubuntu X11/Wayland/GNOME (future adapter templates):** compositor-specific conformance, INCR/selection survival, data-control capability probes, visible degradation, Orca, packaging, and app-pair evidence after M11 promotes Linux.

Every active lane runs `cargo fmt --check`, `cargo clippy -D warnings`, the dependency-direction lint, and the relevant test suite. Only the admitted native lane gates release; compiling shared code on another OS is useful portability feedback, not a support claim.

### Manual matrix
Track irreducible real-host behavior as release evidence: Windows target focus, clipboard contention, elevation boundaries, NVDA, autostart, and SmartScreen are active. macOS Accessibility/Secure Input/notarization and real Wayland compositors become required only when their adapters are promoted.

---

## 4. Native release and packaging checklists

The Windows checklist is active in M10. macOS and Linux remain reusable templates until M11 promotes a real adapter; unchecked template rows are not current release scope.

### macOS
- [ ] Build a universal binary (Apple Silicon + Intel) or document the minimum architecture.
- [ ] Enable the **hardened runtime** (required for Keychain ACL-scoped key access).
- [ ] Code-sign with a Developer ID Application certificate.
- [ ] **Notarize** with Apple (notarytool) and **staple** the ticket so first launch has no Gatekeeper "damaged / cannot be opened" dialog.
- [ ] Ship the Accessibility-permission onboarding deep-link (paste-back depends on it); ship copy-only degradation when denied.
- [ ] Distribute as a `.dmg` (and optionally a Homebrew cask); register the LaunchAgent/SMAppService for autostart.
- [ ] Verify on a clean macOS install that the app launches, captures, and pastes back without manual `xattr` workarounds.

### Windows
- [ ] **Code-sign** the binary and installer with an EV (or OV) certificate to avoid SmartScreen blocks.
- [ ] Build an installer (MSI/MSIX or an NSIS/Inno installer) that registers the Run-key autostart, the named-pipe permissions, and the `vbuff://` handler.
- [ ] Verify Credential Manager key storage works in an interactive session and DPAPI fallback works for a headless/service principal.
- [ ] Confirm `AddClipboardFormatListener` registration and the message-only window survive across sessions; no `SetClipboardViewer` chain.
- [ ] Verify on a clean Windows install: no SmartScreen warning after signing, capture and paste-back work.

### Linux
- [ ] Build the **dual-compiled** binary linking both X11 and Wayland client libs (one artifact runs under either session).
- [ ] **.deb** and **.rpm** packages with correct runtime dependencies (XCB/XFIXES, wayland-client, D-Bus/Secret Service, tray/StatusNotifier host), an XDG autostart entry, and a desktop file with the `vbuff://` handler.
- [ ] **AppImage** for distro-agnostic distribution (bundle the dual X11/Wayland deps).
- [ ] **Flatpak** with the appropriate portal permissions (GlobalShortcuts portal for Wayland hotkeys, Secret Service for keys); document the sandbox capability limits.
- [ ] Document per-compositor setup: GNOME-Wayland degradation (capture-on-summon + manual hotkey), GlobalShortcuts portal requirement, Secret-Service-daemon-absent encrypted-file fallback.
- [ ] Verify on GNOME-Wayland, KDE-Wayland (wlr), and an X11 session that the right backend is selected and the capability badges are honest.

---

## 5. Dependency ordering and completion gates

### What blocks what
The dependency direction is strictly downward, matching architecture.md, so the build order is forced:

1. `vbuff-types` blocks everything (every crate serializes `Clip`/`Flavor`). **(M1)**
2. `vbuff-store` depends on `vbuff-types`; `vbuff-core` depends on `vbuff-types` and the *store traits* (not the impl). **(M1, M2)** - M2 can proceed against `FakeStore` before the real store is wired into the loop, but the real store must exist for M4.
3. `vbuff-platform` traits + `MockBackend` block the end-to-end loop and the daemon policy tests. **(M3)** - the first real backend blocks the first end-to-end loop.
4. `vbuff-gui` depends on `vbuff-core` + `vbuff-store` + `vbuff-platform`; the end-to-end loop (M4) needs all four plus the root binary. **(M4)**
5. The first OS backend (M3) is a prerequisite for M4 and the first-OS beta. A second native backend is required before any cross-platform claim; the remaining backends do not hold the first useful release hostage.
6. `vbuff-daemon` and live `vbuff-ipc` are extracted only after the single-process loop is proven and a second live client exists. **(M7)** - the daemon split is a demand-driven refactor, not pre-beta scope.
7. `vbuff-cli` is a pure IPC client: it **cannot exist before `vbuff-ipc`** (M7 blocks M8).
8. `vbuff-sync` remains frozen until the first native beta passes its demand gate. If activated, explicit directed handoff (M9) precedes every replication, relay, or CRDT path.
9. The cosmic-text content layer, full a11y/i18n, and first-OS packaging (M10) close the native release; off-LAN or ambient sync cannot become a release gate before handoff evidence exists.

Within a milestone, the watcher thread and store actor must exist before the GUI can read live items; the capture gate (M2) must exist before the watcher (M3/M4) is allowed to persist anything; the canary-grep test (M1) must be green before any real clipboard data is captured (M4).

### First-OS beta gate
Ship the single-process Windows beta only when **all** hold:
- The declared Windows 11 app/format/session matrix passes the copy -> store -> hotkey -> popup -> target-confirmed paste or explicit copy-only loop. Shared traits and capability-model CI are supplemental, not platform evidence.
- The **whole-artifact canary test** is green for DB, WAL, SHM, CAS, temp migration, export, log, and controlled crash artifacts; encryption and key lifecycle are proven on Windows.
- **Cold-start**: hotkey and tray live within a few hundred ms of process start.
- **Capture observability**: a tight N-copy loop yields exactly N entries on event-driven backends. On polling backends, every observed state is captured once, counter jumps are recorded as gaps, and the UI never reports unknowable overwritten states as captured; one copied image still yields exactly one entry with no echo loop.
- **Zero-leak**: 100% of OS-flagged/concealed clips and default-deny-list-app copies never reach `vbuff-store` (tested).
- **Search latency**: paged full-history retrieval reaches records older than the first 1,000 and stays sub-frame at 50k+ items without SQLite work on the egui thread.
- Pinned/permanent items survive clear-all, cap pressure, and restart; manual order preserved.
- **Demand**: of 20 target users, at least 12 activate without documentation, 8 use vbuff four days per week after 30 days, and 5 are willing to pay at least USD 25. Do not fan out platforms if this fails.

### First native release gate (end of M10)
Ship the Windows release only when the M6 beta gate still holds **and**:
- The admitted Windows feature set, format/app matrix, hardened privacy, crash safety, and upgrade path pass on clean profiles.
- **Search at scale**: sub-frame at 100k items (criterion).
- If M9 was activated by demand, **directed handoff** passes its authentication, expiry, replay, crash, wrong-device, and plaintext-canary gates. Ambient history sync is not a first-release requirement.
- **i18n/a11y**: RTL/CJK/emoji content renders via cosmic-text; NVDA navigates every critical Windows workflow, with keyboard-only and text-scaling evidence.
- **Reliability soak**: multi-day run shows near-0% idle CPU and no memory leak.
- **Packaging**: the signed Windows artifact installs, upgrades, starts at login, recovers, and uninstalls cleanly. Other OS artifacts are not part of this gate.

Future-tier work (generic AI/OCR, mobile peers, team libraries, shared boards) is explicitly out of scope for these gates and for this plan. A scoped context gateway may enter a later plan only after the encryption, native privacy, IPC authorization, prompt-injection, and disclosure-audit gates in the competitive strategy refresh pass.
