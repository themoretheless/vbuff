# vbuff - Implementation Plan

This is the executable, milestone-sequenced build plan for vbuff. It is derived from and subordinate to `architecture.md`, which is canonical for the workspace layout, crate names, the crate stack, the data model, and every technical decision. Where this plan and `architecture.md` ever disagree, `architecture.md` wins; treat such a disagreement as a bug in this document. The decision spine (positioning, pillars, roadmap phases, success metrics, non-goals) is canonical for *what* we ship in which phase and *why*; this plan is canonical for *the order in which an engineer builds it*.

The product wedge, restated so every milestone can be checked against it: vbuff is the first clipboard manager that a macOS + Windows + Linux user can install everywhere, sync privately end-to-end, and actually enjoy using. The four corners no competitor occupies at once are: truly cross-platform, genuinely polished, privately synced, and approachable. Every milestone below must move at least one corner forward without regressing another.

---

## 0. The single-process MVP vs. the full multi-crate workspace

`architecture.md` defines a target Cargo workspace of eleven focused crates plus a root binary:

`vbuff-types`, `vbuff-core`, `vbuff-store`, `vbuff-platform`, `vbuff-daemon`, `vbuff-gui`, `vbuff-ipc`, `vbuff-plugin`, `vbuff-sync`, `vbuff-update`, `vbuff-cli`, and the root `src/main.rs` binary.

The shippable MVP is a deliberate **single-process subset** of that workspace. The target MVP links and runs:

- `vbuff-types` (plain data: `Clip`, `Flavor`, `ContentKind`, `ClipMeta`, ids)
- `vbuff-core` (engine: dedup, eviction, retention, capture-gate policy, search, classify)
- `vbuff-store` (currently bundled SQLite schema v5 + FTS5 + migrations + blob CAS + eligible local embeddings + WAL; SQLCipher and OS-keystore integration are still required)
- `vbuff-platform` (the four backend traits + per-OS impls + `MockBackend`)
- `vbuff-gui` (eframe popup + settings viewports, the egui hot path)
- `vbuff-update` (signed release contracts and the offline checksum verifier; network fetch/install remains deferred)
- the root binary (`src/main.rs`): single-instance guard, role dispatch, wiring

The later crates are explicitly deferred:

- `vbuff-daemon` - the MVP wires the watcher/store/core/GUI directly inside the root binary on background threads; a dedicated daemon-orchestration crate that owns a tokio runtime, supervised threads, IPC server, and sync sockets is extracted later (M7). The capture watcher still runs on its own dedicated thread in the MVP; what is deferred is the *crate boundary* and the tokio-based service host, not background capture itself.
- `vbuff-ipc` - the framed control-socket protocol and client/server; deferred to M7. The MVP still needs a single-instance guard (bind-or-forward) but its forwarded intent is minimal (just "show popup"), not the full IPC verb surface.
- `vbuff-cli` - the `vbuff` verb surface (`list`/`get`/`copy`/`add`/`search`/`delete`, `--json`, completions); deferred to M8. It is a pure IPC client, so it cannot exist before `vbuff-ipc`.
- `vbuff-sync` runtime integration - the crate now contains tested CRDT/HLC, envelope, membership, policy, anti-entropy, recovery, receipt, capability, and padding foundations. Discovery, authenticated transport, pairing UI, durable replication, and app wiring remain deferred to M9 (v1) and M11 (v2 transports).
- `vbuff-plugin` runtime integration - manifests, typed transforms, migrations, offline evidence, signed packs, and four curated bounded recipes are tested foundations; Wasmtime execution and installation remain deferred.

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
| **M6** | Remaining OS backends (Win, X11, Wayland-wlr, GNOME fallback) | MVP | The end-to-end loop passes the full per-OS CI matrix | `vbuff-platform` | yes |
| **M7** | Daemon + IPC extraction | v1 | Extract `vbuff-daemon` + `vbuff-ipc`; supervised capture, health, single-instance handoff | `vbuff-daemon`, `vbuff-ipc` | no (crate split) |
| **M8** | CLI + scripting + rich types/search/organization (v1 depth) | v1 | `vbuff-cli`, external pickers, files/custom-MIME, FTS5 on, fuzzy/regex, tags/collections | `vbuff-cli`, `vbuff-core`, `vbuff-store`, `vbuff-gui` | no |
| **M9** | LAN P2P sync | v1 | mDNS discovery, SAS/QR pairing, Noise/TLS transport, encrypted replication between 2 devices | `vbuff-sync` | no |
| **M10** | v1 hardening, i18n/a11y, release engineering | v1 | cosmic-text content layer, full a11y, packaging+signing for all OSes, v1 gate | all + packaging | no |
| **M11** | v2 networked breadth | v2 | Off-LAN relay/cloud-drive transport, conflict resolution, directed push, QR handoff | `vbuff-sync`, `vbuff-cli`, `vbuff-gui` | no |

M0-M6 deliver the single-process MVP. M7-M10 deliver v1 (the multi-crate split happens at M7). M11 is v2. Future-tier work (AI actions, OCR, mobile peers, team libraries, shared boards) is explicitly out of scope here.

The expanded 600-idea backlog is reference material, not an implicit scope increase: engineering ideas 1-113 live in `architecture.md`, product/strategy ideas 114-197 live in `recommendation.md`, user-facing/operations ideas 198-300 live in `docs/ideas-top-300.md`, extended ideas 301-400 live in `docs/ideas-301-400.md`, review backlog items 401-500 live in `docs/ideas-401-500.md`, and evidence-backed ideas 501-600 live in `docs/ideas-501-600.md`. The 100-repository and primary-source evidence catalog lives in `docs/repositories-research-100.md`. The milestone gates below decide when an idea becomes planned work; repository popularity alone never promotes one.

| Range | Canonical backlog source |
|---|---|
| 1-113 | [architecture.md](architecture.md) |
| 114-197 | [recommendation.md](recommendation.md) |
| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) |
| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) |
| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) |
| 501-600 | [docs/ideas-501-600.md](docs/ideas-501-600.md) |

The 2026-07-14 research pass found bundled SQLite 3.50.2 inside the old lockfile, within the WAL-reset bug range documented by SQLite. The baseline now uses `rusqlite 0.40.1` / bundled SQLite 3.53.2 with unneeded default features disabled and an integration test that denies affected engine versions. The deeper concurrent writer/checkpoint reproducer remains backlog item 582 rather than silently expanding M1.

Current baseline before the formal M7 crate extraction: the single-process root is divided into `capture`, `history`, `paste`, `commands`, `diagnostics`, `single_instance`, `tray`, `autostart`, `config`, `ask`, `seed_pack`, `verify`, `heartbeat`, `maintenance`, `memory_pressure`, `doctor`, `runtime_metrics`, `logging`, and event-loop `app` modules. Serializable status contracts live in `vbuff-types`; the same crate also owns minimal `ShowPopup`/`Ping` framing. Capture and commands publish through `Diagnostics`; popup/tray consume typed health, capabilities, a content-free privacy ledger, explicit SLO states, notices, expiry/local/incomplete state, and saved/skipped/lost totals. The golden-tested popup keeps History, Trust, and Compose compact; empty history offers explicit local seed packs. Compose owns an ephemeral stack, safe named steps, and merge templates. Hotkey, tray, and second-instance messages wake egui directly, while tiered capture supervision, byte/RSS pressure policy, secret clawback, pre-injection clipboard verification, and maintenance budgets protect the resident loop. Store schema v5 owns FTS5, facets/fingerprints, keyset sessions, transactions, CAS, migration verification, expiry/audits, and fail-closed content-hash-keyed local embeddings. IPC/plugin/sync/update integration contracts are tested, but no daemon listener, third-party Wasmtime host, browser/editor/mobile client, sync transport, or auto-update install loop is enabled. Preserve these boundaries through M0-M6; M7 adds native re-subscribe/restart, the canonical Windows named pipe, live dispatch, and moves stable modules behind daemon/IPC contracts.

### Batch execution overlay

The 600-item list is now executed in sequential batches of 50 without rewriting milestone scope. Each batch must classify every item as runtime, foundation, adapted, native-required, or rejected; complete three review passes; synchronize the four top-level documents; and pass formatting, strict clippy, tests, feature-variant checks, and whitespace review before commit/push.

| Batch | State | Evidence / next gate |
|---|---|---|
| 001-050 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-001-050.md) |
| 051-100 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-051-100.md); native APIs, SQLCipher, daemon dispatch, and Wasmtime remain milestone gates |
| 101-150 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-101-150.md); native OS conformance, release credentials, live updater/sync/plugin paths, and SQLCipher remain gates |
| 151-200 | Reviewed implementation/foundation complete | [Item ledger and three review passes](docs/implementation-batch-151-200.md); models, native integrations, daemon dispatch, SQLCipher, real compositor evidence, dogfood, and live sync remain gates |
| 201-600 | Queued in groups of 50 | Follow the shared range map and existing milestone ownership |

Batch completion does not override milestone acceptance. For example, item 163 can seal an embedding artifact while M9 remains open until pairing, authenticated transport, persistence, policy, and two-device replication work end to end. Likewise, SQLCipher remains an M1/M4 release blocker despite the broader schema v5 work already landed. Batch 151-200 adds [registered decision gates](docs/decision-gates-151-200.md) and a [v1 data-contract freeze](docs/data-contract-v1.md); missing real-world evidence remains Unknown rather than passing by documentation.

From the first resident milestone onward, every milestone records the same SLO budget: zero unaccounted loss, search p99 at or below 16 ms, idle CPU at or below 0.5%, and login-ready at or below 500 ms. `Unknown` is a release blocker. Scope is similarly mechanical: more than nine current workspace crates, more than one added MVP milestone, or one milestone open beyond 42 days forces a cut-line review before new work starts.

---

## 2. Milestones in detail

For each milestone: **Goal**, **Phase**, **Crates/modules touched**, **Task checklist**, **Acceptance criteria**, **Feature tiers delivered**, and **Pitfalls guarded**. Pitfall references use the catalog in `docs/mistakes-top-500.md` (numbered 1-N within its 18 categories) and the consolidated 25-row competitor-mistakes table in `architecture.md`; where a pitfall is the same item viewed twice, both are cited.

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

**Feature tiers delivered.** None user-facing. Foundation for all tiers.

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
- [ ] Write the **canary-grep at-rest test**: write `CANARY_SECRET`, close the DB, grep the raw `.db`/`-wal`/`-shm`/blob files for zero hits. Wire it into all three OS CI lanes.

**Acceptance criteria.**
- Schema v1 creates cleanly; migration harness applies a checked-in fixture from a synthetic v0 to v1 with zero data loss; refusing-newer-version path tested.
- Round-trip byte fidelity proptest: invalid UTF-8, NUL bytes, CRLF, trailing newlines, RTL, emoji, 4-byte codepoints survive store -> load unchanged.
- Canary-grep test green on macOS, Windows, Linux: no plaintext canary in any on-disk artifact.
- Golden content-hash vector matches; identical flavor sets produce identical hashes; any byte change changes the hash.
- WAL crash-recovery test: SIGKILL mid-transaction, reopen, last committed clip present, `integrity_check` clean.

**Feature tiers delivered.** MVP storage substrate: encrypt-at-rest, transactional-per-capture durability, content-hash dedup key, out-of-row blob CAS (the v1 perf feature, but the store is built for it now), FTS5 schema present (substring tier shipped later), schema migration on upgrade.

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

**Feature tiers delivered.** MVP: dedup-by-hash + move-to-top, count cap with pin exemption, incognito/pause, concealed-flag policy, app blacklist policy, whitespace skip, content-type detection/labeling (the `[MVP]` extras items 7, 8), MVP snippets (date/time placeholders), MVP transforms scaffolding (case/trim/strip live in `vbuff-core` as pure transforms).

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

**Decision.** Retire the highest-risk backend first: Linux Wayland. The production probe covers sway/wlroots data control, KDE, and the GNOME capture-on-summon degradation path before an easier backend expands the matrix. This does not manufacture parity from capability models: the real-session gate in [decision-gates-151-200.md](docs/decision-gates-151-200.md) must produce an explicit support decision. The MockBackend and conformance battery remain OS-agnostic.

**Task checklist.**
- [ ] Finalize the four traits verbatim from architecture.md: `ClipboardBackend` (`run(sink, ctl)` / `read_all` / `write` / `is_concealed` / `capabilities`), `HotkeyBackend` (`register`/`unregister`/`events`/`is_available`), `PasteBackend` (`capture_focus` -> `FocusToken`, `paste_into`, `type_text`, `capabilities`), `TrayBackend` (`install`/`update`/`events`). Add the adjacent `SecretStoreBackend` and `AutostartBackend` traits.
- [ ] Define the shared event/value types: `CaptureEvent`, `Flavor`/`FormatKey`, `Sensitivity`, `SourceApp`, `Control { Pause|Resume|SnapshotNow|Shutdown }`, `PasteCaps`, `Conflict`, `Capabilities`.
- [ ] Keep the **format-mapping table** and checked-in `format-fidelity-v1` corpus as the shared pre-backend oracle (UTI / CF_* / MIME <-> `FormatKey`), preserving unknown custom identifiers and byte-identical round trips.
- [ ] Run `scripts/wayland-reality-check.sh` plus manual copy/hotkey/focused-paste checks on real GNOME, KDE, and sway sessions; record `full`, `capture_on_summon`, or `unsupported` per environment before claiming first-class support.
- [ ] Implement the selected Linux `ClipboardBackend`: native data-control where exposed, XWayland coexistence and dedup, eager multi-flavor realization, explicit unknown provenance, and capture-on-summon rather than fabricated background capture on GNOME.
- [ ] Implement Wayland hotkey/paste capability ladders: GlobalShortcuts portal where available, safe injection only when proven, otherwise visible manual summon or copy-only behavior. Keep tray/autostart/key-provider decisions behind narrow adapters.
- [ ] Implement `MockBackend`: a scriptable `ClipboardBackend`/`PasteBackend`/`HotkeyBackend`/`TrayBackend` emitting scripted `CaptureEvent`s and recording writes, with zero OS dependency, for driving daemon policy in CI.
- [ ] Write the **trait-conformance battery**: a parameterized `#[test]` suite runnable against any backend impl (Mock now, real backends as they land), asserting read-all returns every offered flavor, self-writes are suppressed, capabilities are reported honestly, and `capture_focus` precedes `write` on paste.
- [ ] Execute the documented **build-versus-buy ladder**: verify `tray-icon`, the `global-hotkey` Wayland gap, portal support, keyring/SQLCipher integration, and the native crates against pinned versions; use/wrap/fork/degrade only at the registered trigger.

**Acceptance criteria.**
- The conformance battery passes against `MockBackend` and the selected Wayland backend; real GNOME/KDE/sway evidence is attached and capability-model CI remains supplemental.
- Format round-trip tests pass; an unknown UTI survives as `Custom(id)` byte-for-byte.
- First-session idle CPU is within budget over a multi-hour run; a rapid-copy loop yields one entry per distinct change with no misses or echo duplicates.
- The crate-maturity spike report is committed; any swapped crate is reflected in pinned versions.

**Feature tiers delivered.** MVP platform set (first risk-retiring Linux scope): honest capture monitoring or capture-on-summon, portal/manual hotkey behavior, proven paste or copy-only fallback, tray UI, key access, and autostart on one documented session class.

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
- [ ] Pull packaging left: install the first artifact on a clean environment and run doctor/verification before backend fan-out; source-checkout-only success does not pass M4.

**Acceptance criteria.**
- On the selected first scope: copy in app A, summon the popup through the documented capability path, filter, press Enter, and content lands in app A or the UI explicitly selects copy-only behavior.
- The whole flow is mouse-free.
- Cold-start: hotkey live < a few hundred ms after launch.
- Search-as-you-type stays under ~16 ms/frame at 50,000+ seeded items (virtualized, keyset-paged, no `SELECT *`).
- Restored/self-written clips do not create new rows (suppression confirmed in the live loop).

**Feature tiers delivered.** MVP core: the headline copy->store->hotkey->popup->paste-back loop; substring search-as-you-type with highlight; recency default; keyboard nav; paste-back (plain/rich); tray item; popup near cursor; thumbnails; pin/favorite badges; restore-session-on-launch.

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
- Before M6 fan-out, complete a 14-day first-scope dogfood window as the only clipboard manager with zero silent-loss incidents and zero wrong-target pastes; missing evidence blocks fan-out.

**Feature tiers delivered.** MVP security/privacy set (encrypt-at-rest, skip password fields, honor concealed markers, per-app exclusion, incognito, auto-clear-on-timer, wipe-on-demand, local-by-default, zero telemetry); MVP organization set; MVP settings set; MVP snippets set; MVP transforms set. Extras items tagged `[MVP]`: 34 (strip formatting), 49 (set phrases/snippet bank), 57 (app exclusion), 59 (auto-search quick paste), 69 (paste-as), 79 (per-app exclusion rules), 80 (type-ahead picker), 106 (paste as plain text), 122 (pinned persisting as snippet bank).

**Pitfalls guarded.**
- Secrets into history / no default exclusion / OTP capture / unverifiable trust (`mistakes-top-500.md` section 10; pain-points security): default deny-list + secure-field skip + secret detectors + masked sensitive + zero telemetry.
- Settings silently reset/corrupted (74): versioned, atomically-written config with last-good restore.
- Hotkey conflicts / silently stops working (section 12; pain-points Windows "failed to set hotkey"): bind-time conflict probe, refuse-and-explain.
- Wrong formatting on paste / paste-as-plain collisions (pain-points macoS #109/#1232): explicit plain-vs-rich and conflict-free transform shortcuts.
- Treating capture as best-effort with no observability (36; architecture table #21): visible capture-health state.
- Residual data after clear-all (55): clear removes DB, sidecars, blobs, thumbs, index, then VACUUM.

---

### M6 - Remaining OS/session backends and parity

**Goal.** Bring the proven first-scope loop and MVP feature set to the remaining macOS, Windows, X11, and Wayland session classes, retaining the honest GNOME degradation path, and pass the full per-OS matrix.

**Phase.** MVP.

**Crates/modules touched.** `vbuff-platform` (`windows/`, `linux/x11/`, `linux/wayland/`, the runtime `backends()` Linux selector); the conformance battery runs against each.

**Task checklist.**
- [ ] **Windows `ClipboardBackend`:** message-only `HWND_MESSAGE` window + `AddClipboardFormatListener` -> `WM_CLIPBOARDUPDATE` (event-driven, never `SetClipboardViewer`); `EnumClipboardFormats`/`GetClipboardData`/`GlobalLock`; CF_HTML header parse; honor `ExcludeClipboardContentFromMonitorProcessing`/`CanIncludeInClipboardHistory`; `"vbuffOwnWrite"` sentinel; bounded retry/backoff on `OpenClipboard`; source app via `GetForegroundWindow`->PID->exe; debounce 40-80 ms.
- [ ] **Windows `HotkeyBackend`** (`RegisterHotKey`), `PasteBackend` (`SetForegroundWindow` + `SendInput` Ctrl+V), `TrayBackend` (`Shell_NotifyIcon`), `SecretStoreBackend` (Credential Manager + DPAPI fallback for headless), `AutostartBackend` (Run key).
- [ ] **X11 `ClipboardBackend`:** XFIXES `SelectionNotify` on `CLIPBOARD`; `XConvertSelection(TARGETS)` then per-target convert; INCR protocol for large transfers; **take CLIPBOARD ownership and re-serve on SelectionRequest** so content survives source exit (the persistence role); a dedicated hidden persistent window decoupled from UI; PRIMARY opt-in + hard debounce; source via `_NET_ACTIVE_WINDOW`/`WM_CLASS`; password-manager hint atoms. Use `x11rb` + `xfixes`.
- [ ] **X11 `HotkeyBackend`** (`XGrabKey`), `PasteBackend` (`XTEST` Ctrl+V), shared `TrayBackend` (`StatusNotifierItem`/AppIndicator with XEmbed fallback).
- [ ] **Wayland (wlr) `ClipboardBackend`:** `zwlr_data_control_manager_v1` selection events + per-type `receive` pipes; image-type preference over html; sensitive flag; PRIMARY with debounce. Use `wayland-client` + `wayland-protocols-wlr`; `wl-clipboard` subprocess as the faster-to-ship fallback writer.
- [ ] **Wayland `HotkeyBackend`** via `ashpd` GlobalShortcuts portal; `PasteBackend` via virtual-keyboard/`wtype`/`ydotool`, else `PasteCaps` reports unavailable -> **set-and-let-user-paste**.
- [ ] **GNOME-Wayland fallback:** capability-probe `zwlr_data_control_manager_v1` at startup; if absent, degrade to **capture-on-summon** + manual capture hotkey, with a one-time explainer and a per-platform capability badge in settings ("App exclusion: unavailable on this Wayland session; content-pattern rules still apply"). Never fake capability.
- [ ] **Runtime Linux selection:** `detect_session()` on `XDG_SESSION_TYPE`/`WAYLAND_DISPLAY`; the dual-compiled binary links both X11 and Wayland libs; probe Wayland caps and dispatch.
- [ ] X11<->Wayland<->XWayland bridging: watch both worlds where present, dedup across by content fingerprint.
- [ ] Fill the CI lanes: Windows runner; Ubuntu-X11 via `Xvfb`; Ubuntu-Wayland via headless `sway`; a GNOME-Wayland lane (or a documented capability-probe test) asserting the fallback engages.

**Acceptance criteria.**
- The end-to-end loop and the M5 feature set pass on macOS, Windows, X11, and Wayland-with-wlr in CI.
- Linux survival test: copy, close the source app, the clip is still pasteable (X11 ownership + Wayland materialize-on-event).
- GNOME-Wayland: the capability probe degrades to capture-on-summon with the explainer; no silent empty history.
- Cross-world test: copy in a Wayland-native app, paste in an XWayland app and vice versa, no duplicate entry.
- Canary-grep at-rest test green on all three OSes (re-run; M1 covers the store, this confirms per-OS file paths).

**Feature tiers delivered.** Completes the MVP platform set across all targets: per-OS paste-back, hotkey registration, capture monitoring, tray UI, X11-vs-Wayland auto-detect, Wayland capability detection, clipboard-persistence daemon behavior (X11 ownership), window-class ignore rules, DPI awareness.

**Pitfalls guarded.**
- Clip lost when source closes on X11/Wayland (`mistakes-top-500.md` 4; architecture table #4): eager materialize + X11 ownership.
- Relying on a protocol GNOME refuses / capture only while window open / XWayland window-close stops capture (5, 6, 29; architecture table #5, #6): capability probe + honest fallback + dedicated hidden persistent window.
- PRIMARY vs CLIPBOARD confusion / flooding (11, 12; architecture table #22): distinct sources, CLIPBOARD default, PRIMARY opt-in + debounce.
- Broken viewer chain / two managers double-capture / capture stops when another process owns clipboard (23, 24, 77, 78; architecture table #6, #21): modern `AddClipboardFormatListener`, fingerprint idempotency, tolerate coexisting tools.
- Paste lands in wrong window / auto-paste broken on Wayland (pain-points Linux; architecture table #17): confirm target, and on Wayland honestly fall back to set-and-let-user-paste.
- X11/Wayland/XWayland split not bridged (28; architecture table #25): watch both worlds, dedup across.
- Image arrives as html not pixels (14): prefer the binary image type on Wayland.

**>>> MVP COMPLETION GATE here (see section 5).**

**M6 -> M7 data-contract gate.** Before daemon/CLI/sync consumers attach, freeze the on-disk schema, content-hash vector, native format keys, and IPC serde representation through [data-contract-v1.md](docs/data-contract-v1.md) and executable golden fixtures. Any later break requires a new version, migration/negotiation behavior, and old-reader tests.

---

### M7 - Daemon and IPC extraction

**Goal.** Refactor the proven single-process wiring into the canonical `vbuff-daemon` (supervised threads + tokio runtime, capture health/watchdog, retention timers) and `vbuff-ipc` (framed control-socket protocol, client + server, single-instance handoff). This is where the architecture's daemon/client split becomes real, enabling the CLI and sync.

**Phase.** v1.

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
- The end-to-end loop and full MVP feature set still pass the per-OS matrix after the refactor (no behavior regression).
- IPC contract test: serialize/deserialize every `ClientIntent`/`Response`; two-process single-instance handoff spawns and forwards correctly.
- Watchdog test: simulate a stalled listener, assert re-subscribe and a visible health change.
- Stale-socket-after-crash: unlink and rebind once, verified.

**Feature tiers delivered.** v1 platform/robustness: supervised always-on capture, capture-health observability, single-instance handoff hardening, daemon control socket (the substrate the CLI needs).

**Pitfalls guarded.**
- Capture monitor crash/hang takes down recording silently / capture stops when another process owns clipboard (`mistakes-top-500.md` 22, 36, 77; architecture table #21): watchdog + re-subscribe + visible health.
- Two instances launched / stale socket (architecture failure-modes table): bind-fail forward, unlink-rebind-once.

---

### M8 - CLI, scripting, and v1 depth (rich types, search, organization)

**Goal.** Ship `vbuff-cli` and the external-integration surface, plus the v1 depth across capture types, search, and organization that turns the MVP into a power user's daily driver.

**Phase.** v1.

**Crates/modules touched.** New `vbuff-cli` (pure IPC client); `vbuff-core` (fuzzy/regex search, more transforms, content-type detection); `vbuff-store` (FTS5 switched on, files/custom-MIME, tags/collections, total-size cap, thumbnails, export/import); `vbuff-gui` (filter chips, tags/collections UI, preview pane, paste-stack); `vbuff-ipc` (verb expansion); `vbuff-platform` (files/custom-MIME, PRIMARY toggle, manual capture hotkey).

**Task checklist.**
- [ ] `vbuff-cli`: verbs `list`/`get`/`copy`/`add`/`search`/`delete`; stdin/stdout piping; structured `--json` and NUL-delimited output; shell completions; man pages; external-picker integration (rofi/dmenu/fzf) and the `vbuff://` URL scheme.
- [ ] Capture/types: files/folders (store the native list + metadata, not bytes), custom MIME byte-for-byte, full capture metadata, source-app tagging where the OS allows, secure-input skip, regex/keyword exclusion, per-item size limit, manual capture hotkey, content-type detection/labeling (URL/email/color/code/path), PRIMARY-selection capture toggle.
- [ ] Store/perf: switch FTS5 indexed search ON (substring stays the small-history default), total-size cap, out-of-row blob CAS in production use, thumbnails, list virtualization confirmed at scale, unlimited mode, configurable storage location, backup/restore, JSON/CSV export + lossless import (merge by content hash), owner-contention handling.
- [ ] Search: fuzzy (`nucleo`), regex (bounded with a visible "scanning…" state), FTS relevance ranking (`bm25`), filter by app/date/tag/status/collection, active-filter chips, diacritic normalization, collection-scoped search.
- [ ] Paste: plain/rich default + modifier override, keystroke fallback, terminal-safe combo, paste-stack/FIFO multi-paste, merge/append, paste-and-delete, drag-and-drop, tray-paste, restore-clipboard-after-paste.
- [ ] Organization: tags, folders/collections, named tabs, pinboards, color-code, notes, custom display name, multi-select bulk organize, sort controls, numbered quick-slots (Cmd/Ctrl+1-9,0), move-between-collections, smart duplicate-merge.
- [ ] Transformations (v1): programmer-case convert, regex find&replace, base64/URL encode-decode, JSON pretty-print/minify, sort/dedup lines, color format conversion, bind hotkeys to transformations. (Run-shell-command and user scripting stay v2.)

**Acceptance criteria.**
- `vbuff list --json | jq` works; `echo foo | vbuff add` adds a clip; rofi/dmenu picker pastes back.
- FTS5 search stays sub-frame at 100k items (criterion benchmark); fuzzy and regex tiers return correct, bounded results.
- A file copy stores the URI list + metadata, not the bytes; a custom-MIME clip round-trips byte-for-byte.
- Export then import round-trips content, pins, order, timestamps, types losslessly with no duplicate rows.
- Bulk delete is a single transaction that GC's blobs and updates the index.

**Feature tiers delivered.** v1 capture/types, store/perf, search, paste, organization, transformations, and the full CLI/integration set. Extras `[v1]` items including 27 (color convert), 33 (line ops), 46 (tags), 56/69 (paste-as-format), 58 (provenance filtering), 60 (inline editor), 62 (numbered quick-paste), 75/104 (pinboards), 82 (bulk delete by time), 113 (power search), 117 (inline editing), 119 (retention policy).

**Pitfalls guarded.**
- Slow search / full scans / no index at scale (`mistakes-top-500.md` 64, section 3; architecture table #16): FTS5 + virtualized keyset list.
- No export/import / no migration between machines / backup leaks secrets (57, 58, 59-storage): documented JSON export+import, sensitive excluded by default, encrypted export option.
- Reading only text/plain / files-as-text / custom-MIME dropped (13, 67; architecture table #10): full multi-flavor capture, file lists and custom MIME preserved.
- Clumsy Linux picker integration (pain-points cliphist): first-class rofi/dmenu/fzf with NUL output to avoid column-split bugs.
- No bulk select/management (79): set-based bulk ops.
- Clip ordering corrupts / pins reorder (65, 40): immutable monotonic recency column separate from user pin order.

---

### M9 - LAN P2P sync

**Goal.** Encrypted local-network sync: device discovery, MITM-resistant pairing, an end-to-end-encrypted transport, and record-level replication that round-trips a clip between two paired devices. This is the heaviest single milestone; it depends on the entire single-machine stack being stable.

**Phase.** v1 (the spine tags some sync features MVP-priority, but they are sequenced first within v1 because they depend on a stable single-machine store).

**Crates/modules touched.** New `vbuff-sync` (mDNS discovery, Noise/TLS transport, pairing, replication); `vbuff-store` (`device`, `sync_log`, Lamport clock, `local_only`/`secret` sync-exclusion); `vbuff-daemon` (sync sockets on the runtime); `vbuff-gui` (device list, pairing UI, SAS/QR).

**Task checklist.**
- [ ] mDNS device discovery on the LAN; device-list management UI.
- [ ] Pairing with a verification code: SAS (short authentication string) compare-code and QR-code pairing; constant-time code compare (`subtle`); store paired-device pubkeys in `device`.
- [ ] In-transit encryption: a Noise handshake (or rustls/TLS) over the LAN socket; relay/peer sees only ciphertext; per-device keys.
- [ ] Record-level replication (never sync the raw DB file): `StoreEvent::Inserted` -> push; Lamport logical clock + deterministic last-writer-wins; per-item `sync_log` state; auto-sync-on-copy.
- [ ] Sync exclusion: `secret`/sensitive items and `local_only` items never leave the device; per-content-type and per-source toggles; sync pinned/favorites option.
- [ ] Sync of rich/typed content (images, files, html, color), not just plain text; large-object transfer reuses the CAS.

**Acceptance criteria.**
- Two paired devices: copy on device A, the clip appears on device B within a few seconds, end-to-end encrypted; a packet capture shows only ciphertext.
- Pairing requires the SAS/QR confirmation; a mismatched code refuses pairing.
- Sensitive and local-only clips never replicate (verified).
- Conflict test: concurrent edits on both devices converge deterministically via Lamport+LWW; duplicate arrivals do not create duplicate rows.

**Feature tiers delivered.** v1 sync foundation: mDNS discovery, LAN P2P replication, in-transit encryption, device-list management, pairing with verification code, auto-sync-on-copy, sync of typed content, sync exclusion. Extras `[v1]` items 83 (directed push - groundwork), 84 (QR pairing), 87 (LAN-only serverless), 88 (E2E), 89 (typed sync), 97 (cross-device pinboards), 98 (auto-skip sensitive on sync), 102 (SAS).

**Pitfalls guarded.**
- Insecure/paywalled/LAN-only/dead-backend sync (`mistakes-top-500.md` section 9; architecture table #19): E2E encrypted, no vendor backend that can be killed, self-host posture.
- DB-on-cloud-folder corruption (53; architecture table #18): record-level sync, never the raw SQLite file.
- Sensitive copies synced by hidden default (pain-points security): sensitive/local-only excluded by construction; sync is explicit.
- Fragile sync that silently stops / conflict files (pain-points; architecture table #18): deterministic Lamport+LWW, per-item sync state, visible sync status.

---

### M10 - v1 hardening, i18n/a11y, release engineering

**Goal.** Close v1: the cosmic-text content-shaping layer, full accessibility and internationalization, performance verification at scale, crash-safety guarantees, and the per-OS release/packaging/signing pipeline. This milestone owns the v1 completion gate.

**Phase.** v1.

**Crates/modules touched.** `vbuff-gui` (cosmic-text galley for clip content, a11y roles, i18n, density/theme polish); `vbuff-store` (crash-recovery, integrity, backup); packaging configs across all crates; CI release lanes.

**Task checklist.**
- [ ] Integrate the `cosmic-text`-backed galley layer for clip *content* text (CJK/Indic/Arabic/emoji/RTL); keep egui chrome on egui. RTL mirroring + BiDi.
- [ ] Accessibility: `accesskit` roles for popup and settings, screen-reader live announcements, focus traps, high-contrast, reduced-motion, keyboard cheat-sheet, onboarding tour.
- [ ] i18n: localization + locale-aware date/time formatting (UI layer only; storage stays UTC epoch-millis), complex-script shaping verified.
- [ ] UI polish: follow-OS-theme, per-type row styling, full preview pane, density toggle, font/size choice, resizable + remembered window, multi-monitor placement, HiDPI crispness, search-result highlighting.
- [ ] Robustness: WAL crash recovery in production, `integrity_check` at startup with quarantine-and-restart, online-backup before migrations, disk-full handling (abort capture txn, pause, notify), CAS orphan GC, owner-contention backoff.
- [ ] Master password (v1): wrap the root DEK with `Argon2id`; quick PIN (rate-limited); idle auto-lock + OS screen-lock signal handling; locked collections; confirm-before-clear; clear-on-exit; masked sensitive clips; private-browsing exclusion where the OS allows.
- [ ] Performance verification: criterion benchmarks for sub-frame search at 100k items, insert/evict throughput, cold-start; multi-day idle-CPU-near-0% soak; memory bounded (no linear leak).
- [ ] **Per-OS release/packaging/signing** (see section 4): macOS notarization + hardened runtime; Windows code signing + installer; Linux .deb/.rpm/AppImage/Flatpak.

**Acceptance criteria.**
- RTL/CJK/emoji clip content renders correctly via cosmic-text; a screen reader navigates the popup.
- Sub-frame search holds at 100k items; multi-day soak shows no memory leak and near-0% idle CPU.
- Master-password wrap/unwrap works; wrong password fails without wiping; password change preserves DB readability.
- Signed/notarized artifacts install cleanly on a fresh machine per OS; no "damaged / cannot be opened" Gatekeeper dialog; no SmartScreen block.

**Feature tiers delivered.** Full v1 UI/i18n/a11y set, security/privacy hardening set, performance/reliability/data-integrity set, and the packaging/distribution set. Extras `[v1]` items 119 (retention), 121 (highlight), plus 120 (resizable window) and 116 (rich previews) land here or trail into v2.

**Pitfalls guarded.**
- egui weak BiDi/complex-script blocks RTL/CJK/Indic users (architecture risk table): cosmic-text content layer.
- Performance: linear memory leak / clearing doesn't free / idle CPU spin / whole-history render (`mistakes-top-500.md` section 16; pain-points performance): bounded working set, virtualization, event-driven idle.
- Accessibility/i18n gaps (section 17): accesskit roles, localization, complex-script shaping.
- Blocked launch via signing/notarization gaps (pain-points business; section 18): proper notarization/signing in the pipeline.
- DB corruption / power loss / non-atomic writes (52, 73, 75; architecture failure modes): WAL + integrity_check + atomic writes + backups.

**>>> v1 COMPLETION GATE here (see section 5).**

---

### M11 - v2 networked breadth

**Goal.** Extend sync beyond the LAN with privacy intact, and add the collaboration-adjacent conveniences the spine assigns to v2.

**Phase.** v2.

**Crates/modules touched.** `vbuff-sync` (off-LAN transports, conflict resolution depth), `vbuff-cli` (sync verbs), `vbuff-gui` (transport choice, directed push, QR handoff), `vbuff-store` (per-item sync exclusion depth).

**Task checklist.**
- [ ] Off-LAN transports: optional cloud relay (relay sees only ciphertext) and user-cloud-drive transport, both opt-in, E2E encrypted; auto-vs-manual sync toggle; push-single-item; send-to-specific-device (directed push).
- [ ] Conflict resolution depth (Lamport + LWW across non-LAN paths); per-content-type sync selection; per-item/source sync exclusion; sync pinned/favorites.
- [ ] Conveniences: shareable clip link, QR display of a single clip (handoff), export-selected-to-file; settings/distribution polish (export/import config, reset-to-defaults, in-app updater, opt-in telemetry, theme/accent/density, popup layout config, language selection, scheduled clearing, installer settings migration).

**Acceptance criteria.**
- A clip syncs between two devices off-LAN via relay or cloud-drive, E2E encrypted, with deterministic conflict resolution; the relay/drive holds only ciphertext.
- Directed push delivers to the chosen device only; QR handoff transfers a single clip to an unpaired camera device.

**Feature tiers delivered.** v2 sync transports and conveniences. Out of scope (future): AI actions/OCR/MCP, mobile peers, team shared libraries with roles/expiry/revocation, shared collaborative boards.

**Pitfalls guarded.**
- Vendor-backend lock-in / dead backend (architecture table #19; pain-points Clipt/1Clipboard): relay is optional and zero-knowledge; never a single killable backend.
- Sync silently fails / undocumented text-only limits (pain-points business): visible sync status, typed-content sync, explicit transport choice.

---

## 3. Testing and CI strategy

The testing posture is inherited from architecture.md's consolidated testing strategy and made concrete per milestone above. The crown jewel is `vbuff-core`: because it depends only on traits, almost all behavior is tested headless on any host.

### Layered test types
- **Unit tests** in every crate for pure functions (gate decisions, transforms, format mapping, classify, fractional indexing).
- **Property tests** (`proptest`) for the invariants that must never break: byte-for-byte fidelity (invalid UTF-8, NUL, CRLF, trailing newlines, RTL, emoji, 4-byte codepoints survive round-trip); pinned/favorite/permanent never evicted under any cap; identical content never two rows; the fail-closed gate invariant (paused/incognito/concealed/secure-input/source-unknown-required always `Skip`).
- **Corpus tests** for secret detectors with tracked precision/recall, failing CI on regression.
- **Golden-vector tests** for the BLAKE3 content hash (changing it silently breaks dedup -> release blocker).
- **Crypto tests:** seal/open round-trip; one-byte tamper of ciphertext/nonce/tag -> AEAD error; wrong key -> failure not garbage; master-password wrap/unwrap; the canary-grep at-rest test (the only test that catches "encryption silently didn't engage"), run on all three OSes.
- **Store tests:** migrations forward-apply on checked-in fixture DBs from each prior schema version; WAL crash-recovery by SIGKILL mid-transaction; disk-full via a small loopback/quota FS; FTS5 latency benchmark at 50k+ rows (< 8 ms for the SQL+map step); FTS correctness (diacritic folding, case-insensitivity, prefix matching, CJK tokenization, highlight offset alignment).
- **Concurrency/contention tests:** writer thread + N reader threads paging/searching; a stress harness writing the clipboard from N threads/processes; assert no `SQLITE_BUSY` escapes, no deadlock, bounded retries, no duplicate rows beyond dedup, snapshot consistency under WAL.
- **IPC contract tests** (from M7): serialize/deserialize every `ClientIntent`/`Response`; two-process single-instance handoff.
- **GUI tests:** filter/highlight/selection extracted into pure functions tested headless; egui rendering smoke-tested via `egui_kittest`; permission degradation injects `PasteCapability::ClipboardOnly` and asserts copy-only fallback.
- **Benchmarks** (`criterion`): type-to-filter latency at 100k items, insert/evict throughput, cold-start.

### Trait mocking
The `MockBackend` (built in M3) implements all four backend traits with scripted `CaptureEvent`s and recorded writes, with zero OS dependency. The **trait-conformance battery** is a parameterized `#[test]` suite runnable against any `ClipboardBackend`/`PasteBackend`/`HotkeyBackend`/`TrayBackend` impl; Mock runs it everywhere, each real backend runs it in its OS lane. This lets daemon policy be driven deterministically in CI and lets a new backend prove conformance before it ships.

### Per-OS CI matrix (incl. X11 / Wayland / GNOME lanes)
- **macOS** runner: full suite + macOS backend conformance + canary-grep.
- **Windows** runner: full suite + Windows backend conformance + canary-grep.
- **Ubuntu-X11** lane: run under `Xvfb`; X11 backend conformance, INCR transfer, selection-ownership survival test.
- **Ubuntu-Wayland (wlr)** lane: run under headless `sway`; wlr-data-control conformance, image-type-over-html, cross-world bridge test.
- **GNOME-Wayland** lane (or a capability-probe equivalent): assert the capability probe degrades to capture-on-summon with the explainer and never records silently into an empty history.
- **Headless host** lane: all `vbuff-core`/`vbuff-store` tests against fakes + `MockBackend` (no OS clipboard at all) - the fastest, broadest signal.

Every lane runs `cargo fmt --check`, `cargo clippy -D warnings`, the dependency-direction lint, the test suite, and (per OS) the canary-grep. The benchmark suite runs on a dedicated lane to catch latency/throughput regressions against the success metrics.

### Manual matrix
For the irreducibly OS-specific bits that headless CI cannot exercise: the real macOS Accessibility prompt, real Secure Input, real Wayland compositors (GNOME vs KDE vs sway), real notarization/SmartScreen behavior on a fresh machine. Tracked as a release checklist, not automated gates.

---

## 4. Per-OS release and packaging checklist

Owned by M10 for v1 and re-run for v2. The structural goal is to never reproduce the "blocked launch via signing/notarization gaps" and "abandonware breaks on each OS" failures.

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
5. The first OS backend (M3) is a prerequisite for M4; the remaining backends (M6) gate the MVP completion (parity across all targets).
6. `vbuff-daemon` and `vbuff-ipc` are extracted only after the single-process loop is proven (M4-M6). **(M7)** - the daemon split is a refactor, not a from-scratch build.
7. `vbuff-cli` is a pure IPC client: it **cannot exist before `vbuff-ipc`** (M7 blocks M8).
8. `vbuff-sync` depends on a stable `vbuff-store` (device/sync_log/Lamport) and the daemon runtime (M7), and is sequenced after the single-machine store is proven. **(M9)** - this is why sync is v1 work despite being spine-tagged MVP-priority: it depends on the entire single-machine stack.
9. The cosmic-text content layer, full a11y/i18n, and packaging (M10) close v1; off-LAN sync transports (M11) close v2 and depend on the M9 sync core.

Within a milestone, the watcher thread and store actor must exist before the GUI can read live items; the capture gate (M2) must exist before the watcher (M3/M4) is allowed to persist anything; the canary-grep test (M1) must be green before any real clipboard data is captured (M4).

### MVP completion gate (end of M6)
Ship the single-process MVP only when **all** hold:
- The copy -> store -> hotkey -> popup -> paste-back loop and the MVP feature set (capture, store, substring search, paste, UI, hotkeys, organization, MVP snippets, MVP transforms, MVP security/privacy, MVP settings, MVP CLI-less daemon control) pass the **per-OS CI matrix**: macOS, Windows, X11, Wayland-with-wlr, plus the GNOME-Wayland degradation path.
- The **canary-grep at-rest test** is green on all three OSes (encryption provably engaged).
- **Cold-start**: hotkey and tray live within a few hundred ms of process start.
- **Zero-loss capture**: a tight N-copy loop yields exactly N entries on every platform; one copied image yields exactly one entry (no echo loop).
- **Zero-leak**: 100% of OS-flagged/concealed clips and default-deny-list-app copies never reach `vbuff-store` (tested).
- **Survival**: on X11 and Wayland, closing the source app leaves the clip pasteable.
- **Search latency**: search-as-you-type stays sub-frame at 50k+ items.
- Pinned/permanent items survive clear-all, cap pressure, and restart; manual order preserved.

### v1 completion gate (end of M10)
Ship v1 only when the MVP gate still holds **and**:
- The full v1 feature set (rich types, deep search, organization, scripting/CLI, hardened privacy, crash-safety) passes the matrix.
- **Search at scale**: sub-frame at 100k items (criterion).
- **LAN sync**: a clip round-trips between two paired devices in a few seconds, with verified SAS/QR pairing and E2E-encrypted transport; the relay/peer sees only ciphertext; sensitive/local-only clips never replicate; concurrent edits converge deterministically.
- **i18n/a11y**: RTL/CJK/emoji content renders via cosmic-text; a screen reader navigates the popup.
- **Reliability soak**: multi-day run shows near-0% idle CPU and no memory leak.
- **Packaging**: signed/notarized artifacts install cleanly on a fresh machine for every OS (macOS notarized+stapled, Windows signed, Linux .deb/.rpm/AppImage/Flatpak).

Future-tier work (AI/OCR/MCP, mobile peers, team libraries, shared boards) is explicitly out of scope for these gates and for this plan.
