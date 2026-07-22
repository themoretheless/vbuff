# vbuff - Architecture & System Design

This document separates the running implementation from the intended architecture. The **Current implementation boundary** below is authoritative for present-tense product claims. Sections headed or introduced as target describe design intent and release gates, not capabilities of the current binary.

## Current implementation boundary

- **UI/runtime:** native desktop `eframe`/`egui` only, with History/Stack/Privacy/Settings surfaces. The web demo, browser UI, WASM binary, WASM CI target, and component-model plugin path have been removed.
- **Clipboard input:** generic `arboard` polling reads text or raw-RGBA image content. It is not a native all-flavor backend and cannot prove source application, concealed/private markers, clipboard generation, provenance, or complete flavor enumeration.
- **Paste/output:** automatic focus restoration and keystroke injection are disabled until a native adapter confirms the intended target immediately before injection. Eligible non-sensitive selections are copy-only and require manual paste. Sensitive copy is blocked when OS or third-party clipboard-history exclusion cannot be proven.
- **Storage/security:** the live schema-v7 store is bundled SQLite without SQLCipher. Strict security mode may block capture while encryption or required native privacy capabilities are unavailable. Crypto contracts elsewhere in the workspace do not encrypt this database. One-time passwords, private keys, recovery codes, and explicit skipped-capture recovery instead use a bounded process-only history lane with hard expiry; it never enters SQLite/import, cannot be pinned or session-protected, and vanishes at process exit.
- **Migration/backup:** migration code may create a temporary owner-only safety copy and use it to recover a failed migration. Cleanup runs only after the upgraded or next-start live store opens fully and passes `quick_check`; a failed open preserves the artifact. It is not a durable rollback copy or a user backup service, and backup metadata APIs do not prove that a backup was created.
- **Configuration/UI truth:** unknown configuration keys fail validation, UI preferences round-trip through the root configuration, and reduced motion inherits the OS preference while unset. History actions say Copy unless the active delivery backend proves automatic Paste; AccessKit roles and theme-aware semantic colors are covered by deterministic tests.
- **Plugins:** the versioned native executable protocol uses bounded, big-endian-length-prefixed JSON pipe frames; manifests, grants, and bundle validation are contracts only. The resident app does not launch plugins, sandbox them, install them, or expose clipboard data to them. Runtime activation is release-gated on a real OS sandbox, host-side capability enforcement, publisher trust, and conformance evidence.

The target system remains durable searchable history with SQLCipher at-rest encryption, native all-flavor capture, target-confirmed paste, and opt-in peer-to-peer sync. Current-versus-target status is tracked in the [batch 001-050](docs/implementation-batch-001-050.md), [batch 051-100](docs/implementation-batch-051-100.md), [batch 101-150](docs/implementation-batch-101-150.md), [batch 151-200](docs/implementation-batch-151-200.md), [batch 201-250](docs/implementation-batch-201-250.md), [batch 251-300](docs/implementation-batch-251-300.md), and [batch 301-350](docs/implementation-batch-301-350.md) ledgers; current sharp edges live in the [public limitation ledger](docs/limitations.md).

> **This document describes the target architecture, not the current state of the repository.** The shipped MVP
> binary today is a single generic `arboard`-polling clipboard backend (text + one image flavor, no concealed-hint
> support, no source-app attribution), an unencrypted plain-SQLite store with no FTS5, and `global-hotkey` (which
> does not cover Wayland) - none of the native per-OS backends, encryption, or FTS5 described below exist yet. See
> [docs/code-audit-top-50.md](docs/code-audit-top-50.md) for the full, file-and-line-grounded list of what diverges
> from this design as of the current commit, so it can be read as an implementation checklist rather than mistaken
> for a status report.

---

## Target goals & non-goals

### Goals

- **Local-first, private by default.** No cloud account, no remote server, no telemetry, no network calls out of the box. The clipboard never leaves the machine unless the user explicitly opts into sync. Encryption at rest is on by default.
- **Capture everything, byte-for-byte.** Every simultaneous flavor of a copy (plain text, HTML, RTF, image, file list, custom MIME) is stored under one history item, without whitespace/newline/encoding normalization, so editor selections (Vim wordwise/linewise/block) and exact payloads round-trip.
- **Sub-frame keyboard-driven recall.** The popup opens near the cursor and filters as-you-type with sub-frame latency over 100k+ items; the whole open -> filter -> navigate -> paste-back flow works without touching the mouse.
- **Fail-closed privacy.** Concealed/secure-input markers, app exclusion, and content rules are honored *before* any byte touches disk. Every uncertainty in "should we capture this?" resolves to *do not capture*.
- **One binary, one daemon per session.** A single resident process owns the clipboard watcher, the SQLite writer, the global hotkey, the tray, and the IPC server; CLI and external tools talk to it over a local socket.
- **Native, fast, three-platform parity.** Native Rust stack with per-OS backends behind common traits; the popup hot path is the same render code everywhere.
- **Durable and recoverable.** WAL + transactional-per-capture persistence so a crash or power loss never loses the most recent clip or corrupts the store; schema migrates forward across upgrades.
- **Scriptable.** A first-class `vbuff` CLI, a control socket, stdin/stdout piping, structured `--json` output, and external-picker (rofi/dmenu/fzf) integration.

### Non-goals

- **Not a cloud product.** No vendor backend holds clipboard data. A cloud relay and user-cloud-drive transport are explicitly *opt-in v2* conveniences, never the default path.
- **Not an Electron/Tauri/webview app.** Synthetic keystroke injection, raw selection/pasteboard access, secure-input detection, and sub-frame search redraw rule out a webview runtime on the hot path.
- **Not a guarantee against a privileged local attacker.** A root/admin user, an attached debugger, or a kernel-level attacker on the same machine is out of scope. We defend against stolen disks, other unprivileged local users, shoulder-surfers, and on-the-wire interception.
- **Not a clipboard *editor* at capture time.** Transformations (case, trim, base64, JSON pretty-print, regex replace) are a paste/quick-action concern, never applied to the stored canonical bytes.
- **No silent data loss.** When a permission is missing or an OS API is unavailable (e.g. Wayland foreground-app identity), the feature degrades visibly with an honest UI explanation rather than pretending to work.

---

## Target system diagram: copy -> monitor -> store -> hotkey -> popup -> paste-back

```
                          ┌──────────────────────────────────────────────────────────────┐
                          │              SINGLE vbuff DAEMON PROCESS (per session)         │
                          │                                                                │
 [ COPY in any app ]      │   ┌───────────────┐                                            │
        │  OS clipboard   │   │ Watcher thread│   (Win: WM_CLIPBOARDUPDATE event           │
        │  changes        │   │  Clipboard_   │    macOS: changeCount poll ~250ms          │
        ▼                 │   │  Backend      │    X11: XFIXES selection-notify            │
   OS clipboard ──────────┼──►│  .subscribe   │    Wayland: ext-data-control; legacy wlr)  │
                          │   └──────┬────────┘                                            │
                          │          │ 1. debounce burst -> one capture                    │
                          │          │ 2. CAPTURE GATE (fail-closed):                      │
                          │          │      paused? incognito? concealed? secure-input?    │
                          │          │      app-blacklist? size cap? whitespace? regex?    │
                          │          │ 3. read_all() every flavor, byte-for-byte           │
                          │          │ 4. BLAKE3 content_hash                              │
                          │          ▼                                                     │
                          │   ┌───────────────┐    dedup by hash:                          │
                          │   │  vbuff-core   │    exists -> bump ts + move-to-top          │
                          │   │  (Engine)     │    else   -> build Clip                     │
                          │   └──────┬────────┘                                            │
                          │          ▼                                                     │
                          │   ┌───────────────┐  WAL, txn-per-capture; FTS5 index;         │
                          │   │ vbuff-store   │  blob spill >256KiB to encrypted CAS;       │
                          │   │ SQLite+SQLCipher│ count/size/time eviction (pins exempt);   │
                          │   │  (1 writer)   │  thumbnails ─────► StoreEvent::Inserted ────┼──► vbuff-sync
                          │   └──────┬────────┘                                            │    (future explicit handoff;
                          │          │ reads via WAL snapshot (r2d2 read pool)             │     frozen before native beta)
                          │          │                                                     │
   [ USER presses ]       │   ┌──────▼────────┐                                            │
   [ global hotkey ]──────┼──►│ HotkeyBackend │ 1. PasteBackend.capture_focus() FIRST      │
                          │   │  .events      │ 2. position near cursor, clamp to work area│
                          │   └──────┬────────┘ 3. show popup (toggle closes)              │
                          │          ▼                                                     │
                          │   ┌───────────────┐  type-to-filter -> core search             │
                          │   │ vbuff-gui     │  (FTS5 / substring / fuzzy)                │
                          │   │ egui popup    │  virtualized list, live highlight,         │
                          │   │ (main thread) │  type/pin/app filters, number quick-pick   │
                          │   └──────┬────────┘                                            │
                          │          │ [ Enter / double-click / digit ]                    │
                          │          ▼  PasteIntent{ clip, plain|rich, consume?, restore? }│
                          │   ┌───────────────┐ 1. (opt) snapshot current clipboard        │
                          │   │ PasteBackend  │ 2. ClipboardBackend.write(flavors)         │
                          │   │               │ 3. dismiss popup                           │
                          │   │               │ 4. restore focus + inject Cmd/Ctrl+V       │
                          │   │               │    (terminal-aware; keystroke fallback;    │
                          │   │               │     Wayland: set-and-let-user-paste)       │
                          │   └──────┬────────┘ 5. (opt) restore clipboard / consume clip  │
                          │          ▼                                                     │
                          └──────────┼──────────────────────────────────────────────────┘
                                     ▼
                          [ content lands in originally focused app ]

  EXTERNAL CLIENTS (separate short-lived processes) ──IPC (UDS / named pipe)──► daemon
    vbuff copy (stdin->AddClip) | vbuff paste <id> | vbuff watch | rofi/dmenu | vbuff:// | tray actions
    All converge on the same core + store + platform calls. The daemon is the single arbiter.
```

---

## Target system overview & module architecture

### Target process model: one binary, two roles, never two daemons

The target ships as a **single Rust binary** that can run in two roles selected at startup: a long-lived **core daemon** (clipboard watcher + store + IPC server + sync) and a **transient UI/CLI client**. This is deliberately *not* a microservice split and *not* a monolithic single-window app.

Why a daemon at all, given a GUI app could embed everything? Three hard requirements force the split:

1. **The watcher must outlive the popup.** The popup is summoned and dismissed dozens of times an hour; the clipboard must be captured continuously, including the moment a copy happens with no window open. On X11 specifically, the clipboard manager must *own the CLIPBOARD selection* after the source app exits or the data is lost (clipboard-manager handoff). That mandates a resident process holding selection ownership, fully decoupled from UI lifecycle.
2. **The CLI and tray must talk to the running watcher**, not spin up a second one. `vbuff copy`, `vbuff paste`, `vbuff watch`, rofi/dmenu integration, the `vbuff://` URL scheme, and "Send clipboard now" all need to reach the live in-memory state and the single SQLite writer. Two processes both opening the DB write-path and both grabbing the global hotkey is a correctness disaster.
3. **Cold-start responsiveness.** The tray icon and global hotkey must be live within a few hundred ms of login, long before any window is built. A daemon that defers GUI construction wins here.

So: **exactly one daemon per user session**, enforced by a single-instance guard. The GUI runs *inside the daemon process* on the main thread (GUI toolkits demand the main thread on macOS and Windows), while capture/store/sync run on background threads. The CLI is a *separate short-lived process* that connects over IPC.

| Role | Process | Threading | Lifetime |
|------|---------|-----------|----------|
| Daemon + GUI | the resident `vbuff` process | GUI on main thread; watcher, store, sync, IPC on background threads/tasks | session-long |
| CLI / external picker | a fresh `vbuff <cmd>` process | trivial | milliseconds |
| `vbuff://` handler, tray-spawned actions | re-exec that forwards to daemon over IPC | trivial | milliseconds |

The GUI is **not** a separate process from the daemon. Splitting them would mean serializing image blobs and search results across IPC on every keystroke, and would double the clipboard-access surface. Keeping GUI in-process means the popup reads the store directly (shared `Arc`), and only *external* clients (CLI, pickers, scripts) pay the IPC cost.

**Why not Electron/Tauri/web UI?** Rejected. We need synthetic keystroke injection, raw selection/pasteboard access, secure-input detection, and sub-frame search redraw. A webview adds an IPC hop on the hot path and a second runtime. The decided stack is native Rust GUI.

**Why no browser/WASM build?** The product is a resident desktop utility, not a website. The supported UI is native `eframe`/`egui`; normal launches open it, login launches use an explicit background mode, and all OS capabilities stay in native adapters. The removed browser demo is historical work, not a shipping or CI surface. The replacement plugin executable protocol is only a contract: no extension process or sandbox is active. A future plugin host may launch native subprocesses only after the sandbox and capability gates in the current-boundary section are satisfied.

#### Single-instance guard with handoff

On launch the binary tries to bind the IPC endpoint (Unix socket at `$XDG_RUNTIME_DIR/vbuff.sock` / macOS `$TMPDIR`, named pipe `\\.\pipe\vbuff-<user>` on Windows). If the bind fails because another instance holds it, this launch is **not** the daemon: it forwards its intent (`ShowPopup`, or the CLI verb) to the running daemon and exits. A double-click on the app icon, or `vbuff` with no args, simply summons the existing popup.

```rust
enum LaunchOutcome {
    /// We bound the socket; we are the daemon. Owns the listener.
    Daemon(IpcListener),
    /// Another daemon is live; we forwarded `intent` and should exit.
    Forwarded,
}

fn acquire_or_forward(intent: ClientIntent) -> io::Result<LaunchOutcome> {
    match IpcListener::try_bind(ipc_endpoint()?) {
        Ok(listener) => Ok(LaunchOutcome::Daemon(listener)),
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            IpcClient::connect(ipc_endpoint()?)?.send(intent)?;
            Ok(LaunchOutcome::Forwarded)
        }
        // Stale socket (daemon crashed): unlink and retry once.
        Err(e) if is_stale_socket(&e) => { remove_stale(); retry_bind() }
        Err(e) => Err(e),
    }
}
```

### GUI toolkit decision: egui/eframe (committed)

**Decision: `egui` via `eframe`.** Rationale tied to this app's specific needs:

- **Immediate-mode fits a search-as-you-type list.** The popup is a high-churn view: every keystroke re-filters thousands of rows with live highlighting. egui rebuilds the frame each tick, so there is no retained widget tree to diff; `ScrollArea::show_rows` gives **row virtualization for free**, rendering only visible rows out of tens of thousands.
- **Trivial custom row rendering.** Per-type icons, color swatches, image thumbnails, and match-highlight spans are immediate draw calls, not custom retained widgets.
- **One render path, three platforms.** egui paints via `wgpu`/`glow`, sidestepping per-OS widget quirks for the popup.

**The honest trade-offs vs `iced`:**

| Concern | egui (chosen) | iced |
|---|---|---|
| Search-list churn | Immediate mode, zero diffing, built-in row virtualization | Elm/retained; virtualization is manual |
| Accessibility | `accesskit` integrated (UIA/AT-SPI/AX) | Also `accesskit`-based; comparable |
| Native look | Draws its own widgets - least native-feeling | Closer to native styling |
| RTL / complex-script shaping | **Weak spot.** Limited BiDi/complex-shaping; needs a `cosmic-text` layer for v2 i18n | Uses `cosmic-text` -> far better out of the box |
| Settings/preferences window | Form-heavy UI is verbose in immediate mode | Retained model suits forms better |

**Net:** egui wins decisively on the *hot path* (the popup), which is the product's core. Its weak spot is the i18n bucket (BiDi, complex shaping). **Mitigation (committed):** render clip **content** text through a `cosmic-text`-backed galley layer while keeping egui chrome for everything else. If complex-script fidelity ever becomes a launch blocker, that is the single biggest reason to revisit iced; we start on egui.

The **settings window** and the **popup** are two `eframe` viewports of the same process. The tray is *not* egui (see `TrayBackend`).

### Workspace and crate layout

A Cargo **workspace** with thin platform crates and a fat core. The cardinal rule: **`vbuff-core` has zero OS-specific code and zero GUI code.** It depends only on backend *traits*, keeping the bulk of logic unit-testable on any host with mock backends.

```
vbuff/
├── Cargo.toml                 # [workspace]
├── crates/
│   ├── vbuff-types/           # plain data: Clip, Flavor, ContentKind, Metadata, ids.
│   │                          #   No deps beyond serde. Shared by every crate incl. CLI/IPC.
│   ├── vbuff-core/            # engine: dedup, eviction, retention, search, redaction rules,
│   │                          #   transformations, snippet expansion. Pure logic + trait calls.
│   ├── vbuff-store/           # SQLite (rusqlite + FTS5), migrations, blob spill, WAL, at-rest crypto.
│   ├── vbuff-platform/        # the trait DEFINITIONS + cfg-gated re-export of the active impls.
│   │   ├── src/lib.rs         #   pub trait ClipboardBackend / HotkeyBackend / PasteBackend / TrayBackend
│   │   ├── macos/             #   #[cfg(target_os="macos")]
│   │   ├── windows/           #   #[cfg(target_os="windows")]
│   │   └── linux/             #   #[cfg(target_os="linux")]  -> x11/ , wayland/ , runtime select
│   ├── vbuff-daemon/          # wires watcher↔store↔core↔sync↔IPC; owns background threads/runtime.
│   ├── vbuff-gui/             # eframe app: popup + settings viewports. Depends on core+store+platform.
│   ├── vbuff-ipc/             # framed protocol (serde) over UDS/named-pipe; client + server.
│   ├── vbuff-plugin/          # Native pipe protocol, manifests, transforms, adapters, signed bundles.
│   ├── vbuff-sync/            # Contract-only identity/crypto/policy; explicit handoff before replication.
│   ├── vbuff-update/          # signed update manifests, build attestations, checksum verification.
│   └── vbuff-cli/             # `vbuff` verbs; pure IPC client. shell completions, --json.
└── src/main.rs                # the single binary: role dispatch (daemon vs forward vs cli verb).
```

`vbuff-store` is split from `vbuff-core` because eviction/retention/dedup *policy* (core) is independent of *persistence* (store), letting us fuzz the policy logic against an in-memory fake store. `vbuff-types` is separate so the CLI and IPC can serialize `Clip` without pulling in rusqlite or egui. Dependency direction is strictly downward; `vbuff-core` never depends on `vbuff-gui`, `vbuff-store`, or `vbuff-platform`'s *impls* (only its *traits*).

The crate tree above is the target workspace. Inside the **current single-process root app**, responsibilities are already split so the target extraction is mechanical rather than a rewrite:

| Current module | One reason to change |
|---|---|
| `src/main.rs` | Startup composition and dependency construction |
| `src/app.rs` | eframe event-loop coordination and command dispatch |
| `src/capture.rs` | Clipboard polling, capture policy, retry-on-store-failure |
| `src/history.rs` | Store mutation plus GUI snapshot publication |
| `src/paste.rs` | Guarded clipboard staging and the future target-confirmation sequence; the generic runtime remains copy-only |
| `src/commands.rs` | Canonical `AppCommand` vocabulary for popup, tray, and hotkey |
| `src/diagnostics.rs` | Redacted publication of capture health and command outcomes |
| `src/single_instance/` | Minimal framed startup handoff, liveness, and stale endpoint recovery; common/Unix/Windows code stays separate |
| `src/tray.rs` | Menu-bar icon, menu state, and menu-event mapping |
| `src/autostart.rs` | Per-OS launch-at-login registration |
| `src/config.rs` | TOML configuration, redacted exchange, and explicit check-summed full handoff |
| `src/ask.rs` | Bounded local eligible-history retrieval; no socket or model host |
| `src/seed_pack.rs` | Explicit local starter clips; no onboarding or persistence policy |
| `src/verify.rs` | Offline release checksum CLI adapter; no network or update installation |
| `src/heartbeat.rs` | Atomic external liveness publication for supervisors |
| `src/maintenance.rs` | Capture-friendly expiry, embeddings, audit, FTS, and CAS maintenance |
| `src/doctor.rs` | Content-free human/JSON startup, store, and security health report |
| `src/memory_pressure.rs` | RSS classification and maintenance/capture pressure response |
| `src/runtime_metrics.rs` | Bounded content-free metrics and crash snapshots |
| `src/logging.rs` | Structured tracing formatter with field-name redaction |
| `crates/vbuff-sync/` | Protocol/crypto foundation only; no discovery or runtime transport yet |
| `crates/vbuff-ipc/` | Versioned/scoped control contracts only; no daemon listener/dispatcher yet |
| `crates/vbuff-plugin/` | Capability/typed-plugin contracts only; no sandboxed subprocess host yet |
| `crates/vbuff-update/` | Signed update/build-verification contracts; no network fetch or installer yet |

### SOLID/DRY decomposition and small reading slices

The implementation should remain learnable as a set of small, single-purpose slices. The current repository already has the workspace split; the next cleanup is to keep the root binary as composition glue instead of a second architecture.

| Slice | Owns | Does not own | First files to read |
|---|---|---|---|
| **Types** | Serializable clip data, flavor bodies, ids, content-kind labels, runtime status/notice contracts, minimal startup intents/responses | SQL, UI state, OS calls, business rules | `crates/vbuff-types/src/lib.rs`, `status.rs`, `ipc.rs` |
| **Core** | Pure dedup, classification, recall, eviction, capture/trust policy, composition, everyday workflows, AI/privacy gates, embeddings, delivery decisions, feedback redaction, reliability, and audit algorithms | `rusqlite`, `egui`, `arboard`, native APIs | `crates/vbuff-core/src/lib.rs`, then one concern under `capture/`, `trust/`, `recall/`, `intelligence/`, or `workflow/everyday.rs` |
| **Store** | SQLite schema, migrations, transactions, FTS/facets/fingerprints, CAS, lifecycle annotations, quarantine/export, audits, clawback, doctor facts, and durable queries | Capture policy, GUI filtering decisions, native clipboard access | `crates/vbuff-store/src/lib.rs`, then `search.rs`, `migration.rs`, `cas.rs`, `data_lifecycle.rs` |
| **Platform** | Clipboard/hotkey/paste traits plus desktop-shell, capability, security, lifecycle, Wayland/Windows decision contracts | Product policy, SQL, visual layout | `crates/vbuff-platform/src/traits.rs`, then `desktop.rs`, `capabilities.rs`, `security.rs`, `lifecycle.rs` |
| **GUI state** | Query text, selection, view visibility, queued UI actions | Clipboard reads/writes, DB mutations | `crates/vbuff-gui/src/state.rs` |
| **GUI view** | Search box, rows, badges, thumbnails, keyboard navigation, media projection, and Trust presentation | Capture loop, retention, config parsing | `crates/vbuff-gui/src/navigation.rs`, `projection.rs`, `media.rs`, `trust_view.rs`, `view.rs`, then `app.rs` |
| **History facade** | Serialize store mutations, own the bounded volatile secret lane, and publish bounded GUI snapshots | SQL schema, capture decisions, widget layout | `src/history.rs` |
| **Capture supervisor** | Poll, read flavors, evaluate the cheap gate, retry failed persistence, publish heartbeat/stalled health | Rendering, SQL details, paste injection | `src/capture.rs` |
| **Paste coordinator** | Enforce atomic sensitive-history-exclusion before any clipboard write, stage eligible flavors, and preserve target-verification contracts for a future native adapter | Searching, storage schema, row rendering | `src/paste.rs` |
| **Command layer** | Shared `Show`, `Paste`, `CopyLatest`, `ClearHistory`, `Pause`, autostart, and `Quit` semantics | UI widget styling, OS-specific event delivery | `src/commands.rs`, dispatched by `src/app.rs` |
| **Startup handoff** | One resident process, length-prefixed startup intents, liveness probes, stale endpoint recovery | Clipboard history verbs, CLI API, GUI rendering | `crates/vbuff-types/src/ipc.rs`, `src/single_instance/mod.rs`, then one transport file |
| **Diagnostics** | Typed `CaptureHealth`, security summary, heartbeat/stalled detection, redacted notices, popup/tray status, and doctor report | Clip payload storage, transform behavior | `crates/vbuff-types/src/status.rs`, `src/diagnostics.rs`, `src/doctor.rs`, `src/capture.rs`, `src/app.rs`, `src/tray.rs` |
| **IPC foundation** | Version/capability negotiation, event filters, scoped tokens, batches, and bounded browser/editor/Vim/automation/MCP/launcher/terminal/webhook contracts | Socket ownership, authentication transport, command execution | `crates/vbuff-ipc/src/lib.rs`, then one file in `integration/` |
| **Plugin foundation** | Versioned native pipe frames, manifests/grants, typed pipelines, bounded import/export adapters, recognizers, signed executable bundles, lockfile, and reviewed recipes | Sandboxed process execution, ambient OS access, installation UI | `crates/vbuff-plugin/src/protocol.rs`, then `manifest.rs`, `recipes.rs`, or one concern module |
| **Sync foundation** | CRDT/HLC, crypto, membership, policy, reconciliation, recovery, receipts, padding, gated embeddings, and device-experience decisions | Network discovery, sockets, pairing screens, durable runtime replication | `crates/vbuff-sync/src/lib.rs`, then the `device_experience.rs` facade and one of `device_experience/policy.rs`, `outbox.rs`, `travel.rs`, or another protocol concern |
| **Update foundation** | Signed manifests, key rotation, downgrade/replay protection, staged rollout, build attestation, and streaming checksums | Network fetch, durable release state, install/rollback, updater UI | `crates/vbuff-update/src/lib.rs`, then `manifest.rs`, `attestation.rs`, and `src/verify.rs` |
| **Operations** | Public limitations, release evidence, maintainer continuity, and scope review rules | Product capability claims without runtime/native proof | `docs/limitations.md`, `docs/maintainer-handoff.md`, `docs/scope-review.md`, `.github/workflows/release-provenance.yml` |

SOLID rules for future edits:

- **Single responsibility:** a module should have one reason to change. If a change touches capture policy, paste sequencing, and row layout at once, the boundaries are wrong.
- **Open/closed:** add OS behavior by implementing platform traits, not by branching through the core or GUI.
- **Liskov/substitution:** mock backends must behave like real backends at the trait boundary; tests should prove the core cannot tell the difference.
- **Interface segregation:** keep narrow traits (`ClipboardBackend`, `HotkeyBackend`, `PasteBackend`, future `TrayBackend`/`EmbeddingBackend`) instead of one god backend.
- **Dependency inversion:** high-level policy depends on abstractions and pure data, never concrete OS crates.
- **DRY:** shared commands, design tokens, capability badges, privacy gate reasons, and retention rules must have one source of truth. Duplication in labels or logic is a bug, not harmless documentation drift.

Current extraction status:

1. **Done:** capture polling and pure cheap-gate rules live in `src/capture.rs`; a failed store write is retried instead of poisoning the last-seen hash.
2. **Done:** `AppCommand` is shared by popup, tray, and hotkey dispatch, including one `ClearHistory` meaning (pinned clips survive).
3. **Adapted:** `PasteCoordinator` owns clipboard staging and verification contracts. The generic runtime does not inject paste at all because it cannot confirm the destination; eligible non-sensitive selections stop at copy-only.
4. **Done:** `History` is the small app-layer writer/snapshot API, so capture and commands do not manipulate the store mutex directly. It also owns the at-most-32-item volatile lane for memory-only secret classes, hard expiry, payload-free bounded tombstones, and volatile undo without allowing those clips into SQLite, import, pinning, or session protection.
5. **Done:** `vbuff-gui::design` owns typography, spacing, control, radius, contrast, semantic-color, and font-independent icon-button tokens; navigation, projection, media, and Privacy rendering are separate modules, while the menu-bar icon is isolated in `src/tray.rs`. Twenty-eight headless WGPU goldens cover normal, minimum, and wide layouts across themes and `1x`/`1.5x`/`2x` DPI; persisted UI preferences and OS-derived reduced motion stay outside widget-local state. The [native egui design review](docs/design-review-native-egui.md) records the ten-role critique, interaction contract, and deferred native evidence gates.
6. **Done:** serializable capture-health/notice contracts live below the GUI in `vbuff-types`; the narrow `Diagnostics` publisher carries worker health and redacted command outcomes to popup/tray without coupling capture policy to rendering.
7. **Done:** the capture worker publishes a monotonic heartbeat; its watchdog surfaces `CaptureHealth::Stalled`, ignores deliberate pause, and allows a later successful read to report recovery.
8. **Done:** the root process performs bind-or-forward before opening storage or registering a hotkey; an OS-released owner lock serializes recovery, a second launch forwards `ShowPopup`, `Ping` proves liveness, and a stale endpoint is removed and rebound once.
9. **Done/foundation:** schema v7 owns FTS5 prose/code indexes, structured facets, SimHash/dHash plus normalized text groups, keyset search, Bloom-assisted exact dedup, transactional refcounted CAS, per-kind/collection retention, lifecycle annotations, backup-evidence metadata, import/blob quarantine, versioned export, legal hold, externally keyed grace-record primitives, expiry, content audits, and eligible local embeddings. The live database remains unencrypted. Migration safety copies are temporary and removed after a successful verification; neither they nor backup-evidence metadata constitute a durable backup service.
10. **Done:** hotkey, tray, and second-instance events wake egui directly; a five-second supervisory repaint replaces the former 100 ms resident poll, while the visible popup uses a one-second refresh for expiry and capture state instead of repainting every frame.
11. **Frozen foundation:** `vbuff-sync` has tested protocol/crypto and focused device-experience modules, but no transport, persistent authenticated identity lifecycle, pairing UI, or runtime integration. It remains inactive until the first native beta passes demand gates; if resumed, one explicit TTL-bound handoff precedes replication.
12. **Done:** tiered capture supervision, byte-aware backpressure, RSS-aware maintenance, secret detection/clawback, doctor output, process hardening, strict posture, FTS health, and atomic store batches are active in the current root runtime.
13. **Foundation:** `vbuff-ipc` and `vbuff-plugin` own bounded browser/editor/Vim/automation/MCP/launcher/terminal/webhook and import/export contracts, but no daemon listener, local HTTP server, signed client, plugin-process host, OS sandbox, or third-party execution is enabled. The executable plugin protocol remains contract-only and release-gated.
14. **Done/foundation:** the popup owns golden-tested History/Stack/Privacy/Settings surfaces; feedback opens only a redacted draft; the local Stack is ephemeral; `vbuff-update` owns signed release contracts; config/ask/verify remain narrow root adapters.
15. **Done/foundation:** capture emits fail-closed `ai_allowed`; the store uses a hot-swappable local embedding boundary in the capture transaction; IPC integration types, recipes, encrypted vector artifacts, format/data freezes, and delivery gates are tested without enabling ambient listeners or models.
16. **Done/foundation:** config schema 2 uses explicit preview/apply and rejects unknown keys on both runtime and migration paths. Migration failure can restore from a temporary safety artifact; cleanup waits for the upgraded or next-start store to open fully and pass `quick_check`, and there is no persistent rollback backup. The popup exposes sticky kind/source filters, profiles, metadata-only health, stale-pin review, plain clones, session protection, and byte alerts; idle/lock auto-pause still requires native continuous signals.
17. **Done/operations:** the limitation ledger, source-bound release evidence workflow, maintainer handoff, and quarterly scope review are versioned; credentials, completed drills, and tag evidence remain external facts.
18. **Done/foundation:** `trust/` owns consent/access/posture/rule/secret decisions, `recall/` owns parsing/ranking/search-memory/source/graph contracts, schema 7 owns lifecycle sidecars, and `desktop.rs` owns shell labels/status/fallback/self-check policy; the ledger marks which pieces reach the resident UI.
19. **Next:** implement and prove native generation/provenance/concealed/all-flavor capture, trustworthy target confirmation, and OS-history exclusion; then collect real Wayland and dogfood evidence. SQLCipher/keystore, the canonical Windows named pipe, and extraction of stable contracts into `vbuff-daemon` remain later gates.

The current startup transport is deliberately smaller than the target IPC service. macOS/Linux use an owner-only Unix domain socket; Windows uses authenticated loopback plus owner-local metadata until the named-pipe backend lands. Both carry only `ShowPopup` and `Ping`, use bounded length-prefixed JSON frames, and keep clipboard data outside the bootstrap channel.

### Core data model

```rust
/// One logical copy event. Preserves item order and every simultaneous flavor.
pub struct Clip {
    pub id: ClipId,                 // ULID: sortable by creation time, sync-friendly
    pub items: Vec<ClipboardItem>,  // item boundaries are part of the clipboard contract
    pub primary_kind: ContentKind,  // detected type for icon/filter (Text/Url/Color/Code/Image/File...)
    pub content_hash: [u8; 32],     // BLAKE3 over canonicalized item/flavor topology -> dedup key
    pub meta: ClipMeta,
    pub pinned: bool,
    pub favorite: bool,
    pub permanent: bool,            // promoted out of the ephemeral/auto-prune pool
}

pub struct ClipboardItem {
    pub ordinal: u32,
    pub flavors: Vec<Flavor>,       // text/plain, text/html, RTF, image/png, uri-list, custom MIME...
}

pub struct Flavor {
    pub mime: MimeType,             // normalized cross-OS MIME (see per-OS UTI/CF_* mapping)
    pub bytes: Body,                // Inline(Vec<u8>) for small, Spilled(BlobRef) for big (out-of-row)
}

pub struct ClipMeta {
    pub created_at: SystemTime,
    pub byte_size: u64,
    pub source_app: Option<AppId>,  // bundle id / exe path / WM_CLASS / app-id
    pub source_app_name: Option<String>,
    pub source_window_title: Option<String>,
    pub sensitive: bool,            // concealed-type / secret-detector / password-field flag
    pub ai_allowed: bool,           // affirmative capture-gate decision; missing/legacy means false
}
```

`content_hash` is the dedup pivot, computed over the **canonical clipboard topology**: ordered item boundaries plus each item's MIME-sorted raw flavors, byte-for-byte, with no text normalization. Re-copying identical topology matches the hash -> the existing row's timestamp is bumped and it moves to top instead of inserting a duplicate. The current Rust type remains a flat `Vec<Flavor>`; migration to this target model is part of the native fidelity gate and must happen before claiming multi-item losslessness.

### The four cross-platform backend traits

All platform variance is funneled through four traits in `vbuff-platform`. The daemon and GUI program **only against these traits**; they never name an OS.

```rust
/// Watch the OS clipboard and read full multi-flavor contents.
pub trait ClipboardBackend: Send {
    fn subscribe(&self, sink: Sender<ClipboardEvent>) -> Result<Subscription>;
    fn read_all(&self) -> Result<CapturedClipboard>;       // every flavor, byte-for-byte
    fn write(&self, items: &[ClipboardItem]) -> Result<()>; // for paste-back
    fn is_concealed(&self) -> bool;                        // skip capture if true
}

/// Register/unregister global hotkeys and deliver presses.
pub trait HotkeyBackend {
    fn register(&self, id: HotkeyId, combo: &KeyCombo) -> Result<()>;
    fn unregister(&self, id: HotkeyId) -> Result<()>;
    fn events(&self) -> Receiver<HotkeyId>;
    fn is_available(&self, combo: &KeyCombo) -> Conflict;  // bind-time conflict probe
}

/// Focus restoration + synthetic paste into the previously focused app.
pub trait PasteBackend: Send {
    fn capture_focus(&self) -> Result<FocusToken>;         // snapshot foreground app FIRST
    fn paste_into(&self, target: &FocusToken, opts: PasteOptions) -> Result<()>;
    fn type_text(&self, target: &FocusToken, text: &str) -> Result<()>; // keystroke fallback
    fn capabilities(&self) -> PasteCaps; // Wayland: injection unavailable -> set-and-let-user-paste
}

/// Tray / menu-bar status item with a recent-items menu.
pub trait TrayBackend: Send {
    fn install(&self, model: TrayModel) -> Result<TrayHandle>;
    fn update(&self, handle: &TrayHandle, model: TrayModel) -> Result<()>;
    fn events(&self) -> Receiver<TrayEvent>; // Open, TogglePause, PasteRecent(idx), Quit...
}
```

`HotkeyBackend` deliberately has no `Send` supertrait. Native registration
managers can own thread-affine event-loop handles (including the Windows
manager used by `global-hotkey`); the owner stays on its creating thread and
forwards only typed hotkey events across channels. Clipboard and paste
backends retain `Send` because the current runtime moves them into workers.

Two adjacent traits round out the platform surface (kept separate so they can degrade independently): `SecretStoreBackend` (Keychain / DPAPI-Credential-Manager / Secret-Service) and `AutostartBackend` (LaunchAgent/SMAppService / Run-key / XDG autostart).

### Compile-time impl selection via `cfg`, runtime selection on Linux

Each trait has one concrete impl per platform, chosen at **compile time** via a `cfg`-gated module tree plus a single `backends()` constructor the daemon calls without knowing the OS.

**Linux is the one place where `cfg` is not enough** - X11 vs Wayland is a *runtime* decision on the same binary. So the Linux module compiles *both* and dispatches at runtime on `XDG_SESSION_TYPE` plus socket probing, then probes Wayland compositor capabilities:

```rust
// crates/vbuff-platform/src/linux/mod.rs  (#[cfg(target_os = "linux")])
pub fn backends() -> Result<Backends> {
    match detect_session() {
        Session::Wayland => {
            let caps = probe_wayland();           // ext/wlr data-control? virtual-keyboard? portal?
            Ok(Backends {
                clipboard: wayland::clipboard(caps)?,   // ext-data-control, legacy wlr, or visible fallback
                hotkey:    wayland::hotkey(caps)?,       // GlobalShortcuts xdg-desktop-portal
                paste:     wayland::paste(caps)?,        // virtual-keyboard / wtype, else set-and-let-user-paste
                tray:      linux::tray()?,               // StatusNotifierItem (shared X11/Wayland)
            })
        }
        Session::X11 => Ok(Backends {
            clipboard: x11::clipboard()?,                // CLIPBOARD selection owner + handoff
            hotkey:    x11::hotkey()?,                   // XGrabKey
            paste:     x11::paste()?,                    // XTEST Ctrl+V
            tray:      linux::tray()?,
        }),
    }
}
```

The dual-compile means the Linux build links both X11 and Wayland client libs. The trade-off (heavier Linux binary, both dep trees compiled) is worth it: one artifact runs under either session, and a user switching login type gets the right backend with no reinstall.

#### Per-OS impl map

| Trait | macOS | Windows | Linux/X11 | Linux/Wayland |
|---|---|---|---|---|
| Clipboard | `NSPasteboard` changeCount **polling** (~150-250 ms; no OS callback) | `AddClipboardFormatListener` -> `WM_CLIPBOARDUPDATE` (event-driven) | own/observe `CLIPBOARD` selection via XFIXES, request `TARGETS`; manager handoff | `ext-data-control-v1` when advertised; deprecated `wlr-data-control` fallback; visible degraded mode otherwise |
| Hotkey | Carbon `RegisterEventHotKey` (or `CGEventTap`) | Win32 `RegisterHotKey` | `XGrabKey` | `GlobalShortcuts` portal (compositors block raw grabs) |
| Paste | restore focus + `CGEvent` Cmd+V; needs Accessibility (`AXIsProcessTrusted`) | `SetForegroundWindow` + `SendInput` Ctrl+V | `XTEST` Ctrl+V | `virtual-keyboard`/`wtype`/`ydotool`, else set-and-let-user-paste |
| Tray | `NSStatusItem` (menu bar) | `Shell_NotifyIcon` | `StatusNotifierItem`/AppIndicator (XEmbed fallback) | same SNI |
| Secret store | Keychain (Security.framework) | Credential Manager + DPAPI | Secret Service (GNOME Keyring/KWallet), encrypted-file fallback |

### Concurrency model

- **Watcher thread** (1, dedicated, OS-native loop): owns the `ClipboardBackend`. On Windows it runs a message pump for `WM_CLIPBOARDUPDATE`; on macOS a `CFRunLoop`/timer for changeCount; on X11/Wayland an event loop. These are *not* tokio-friendly, hence a real thread; communicates inward via `crossbeam`/`mpsc` channels.
- **Store actor** (1, owns the single `rusqlite::Connection`): SQLite is single-writer; all writes funnel through one owner to avoid `SQLITE_BUSY`. Popup reads go through a read-only pooled connection (WAL allows concurrent readers).
- **Daemon tokio runtime**: IPC server, mDNS/sync sockets, retention timers.
- **GUI thread = main thread** (OS requirement). Talks to the store via shared `Arc` + the store actor's channel; never blocks on IO in the frame (search is bounded; large blob loads are async with a placeholder).

Shared state is an `Arc<RwLock<…>>` for hot config (pause flag, blacklist, incognito) read by the watcher every event; everything mutating the DB goes through the store actor's mpsc to serialize writes.

---

## Clipboard capture & monitoring backends

This subsystem observes the OS clipboard, reads every flavor of a copy, attributes it to a source application, and feeds normalized capture events into the storage/dedup pipeline. It is the most platform-divergent part of the app, built around the single `ClipboardBackend` trait boundary with three native implementations.

### Design goals and constraints

- **Event-driven where the OS allows it, polling only where it does not.** Windows and supported Wayland data-control implementations give real change notifications; macOS gives none (poll `changeCount`); X11 is a hybrid (XFIXES selection-notify events, complicated by ownership semantics). Polling can detect a `changeCount` jump but cannot reconstruct overwritten intermediate payloads, so the product reports gaps instead of promising universal zero loss.
- **Capture all flavors atomically.** A single copy offering HTML + RTF + plain text + image is read in one pass and stored under one item. This rules out `arboard` as the primary backend.
- **Byte-for-byte fidelity.** No newline normalization, no re-encoding, no whitespace trimming at the capture layer.
- **Honor sensitivity hints before storage.** Concealed markers, secure-input flags, and excluded apps are checked *before* the payload is persisted or fully read into long-lived memory.
- **Never block the UI and idle near 0% CPU.** Capture runs on a dedicated thread/event loop owned by the daemon.

### arboard vs native: why native wins for capture

`arboard` is the current bring-up backend and polls text or image content. It has no change notification, atomic all-flavor read, RTF/HTML/file-list/custom-MIME enumeration, sensitivity flags, generation proof, or source-app information. **Target decision:** replace it with native capture backends per OS and retain it, if useful, only as an explicitly degraded fallback. Until then, native privacy and fidelity claims remain unavailable. The capture layer is GUI-agnostic and has no dependency on the rendering crate.

### The backend abstraction (policy stays in the daemon)

The daemon owns the backend on its capture thread and receives `CaptureEvent`s over a channel; the daemon (not the backend) applies privacy filters, dedup, size caps, and persistence. Keeping policy out of the backend keeps each platform module small and testable.

```rust
/// A single clipboard flavor as read from the OS, byte-for-byte.
pub struct Flavor {
    pub format: FormatKey,           // normalized vbuff format key
    pub native_id: NativeFormatId,   // raw platform id (UTI / CF_* / MIME target) for round-trip
    pub bytes: Vec<u8>,
}

pub enum FormatKey {
    PlainUtf8, Html, Rtf,
    Image(ImageKind),   // Png, Bmp, Tiff, ...
    FileList,           // text/uri-list / CF_HDROP / NSFilenamesPboardType
    Custom(String),     // arbitrary app MIME, preserved verbatim
}

#[derive(Default)]
pub struct Sensitivity {
    pub concealed: bool,        // org.nspasteboard.ConcealedType, Wayland sensitive
    pub exclude_history: bool,  // Win ExcludeClipboardContentFromMonitorProcessing
    pub transient: bool,        // Win CanIncludeInClipboardHistory == false-ish
    pub exclude_cloud_sync: bool, // Win CanUploadToCloudClipboard == false
}

pub struct CaptureEvent {
    pub seq: u64,               // monotonic per-backend change counter
    pub captured_at: SystemTime,
    pub items: Vec<ClipboardItem>, // ordered item boundaries plus every offered flavor
    pub sensitivity: Sensitivity,
    pub source: SourceApp,      // bundle id / exe / WM_CLASS / app-id, name, title, icon
}

pub trait ClipboardBackend: Send {
    /// Start watching. Pushes coalesced events into `sink`.
    /// `ctl` lets the daemon pause/resume/snapshot without tearing the backend down.
    fn run(&mut self, sink: Sender<CaptureEvent>, ctl: Receiver<Control>) -> Result<(), BackendError>;
    fn capabilities(&self) -> Capabilities;
}

pub enum Control { Pause, Resume, SnapshotNow, Shutdown }
```

The daemon's consume loop owns all *policy*; the backend never decides what to drop:

```rust
fn on_event(&mut self, ev: CaptureEvent) {
    if self.paused || self.incognito { return; }
    if ev.sensitivity.concealed || ev.sensitivity.exclude_history { return; }
    if self.app_blacklist.matches(&ev.source) { return; }
    if let Some(text) = ev.plain_text() {
        if self.whitespace_only_skip && text.trim().is_empty() { return; }
        if self.regex_excludes.iter().any(|r| r.is_match(text)) { return; }
    }
    let hash = blake3::hash(ev.canonical_bytes());
    if let Some(existing) = self.store.find_by_hash(hash) {
        self.store.bump_to_top(existing, ev.captured_at);   // dedup + move-to-top
        return;
    }
    if ev.total_bytes() > self.per_item_cap { /* truncate or reject per policy */ }
    self.store.insert(ev.into_item());            // transactional, WAL
}
```

### Per-OS implementations

#### macOS - NSPasteboard `changeCount` polling

macOS has **no clipboard-change callback**. Poll `[NSPasteboard generalPasteboard].changeCount` on a timer; the integer increments on every system write. Polling at **150-250 ms** is the industry sweet spot (Maccy, Flycut), imperceptible and negligible CPU; back off to ~500 ms when idle/unfocused and on battery.

Read all flavors in one pass: enumerate `pasteboardItems`, then each item's `types` (UTIs). Specifics:
- **Concealed type:** `org.nspasteboard.ConcealedType` (1Password, KeePassXC convention) -> skip. Also respect `org.nspasteboard.TransientType` and `org.nspasteboard.AutoGeneratedType`.
- **Secure Event Input:** `IsSecureEventInputEnabled()` (Carbon) is global; use it to suspend keystroke synthesis (paste-back) and, heuristically, skip capture while active. Best-effort.
- **Source app:** `NSWorkspace.sharedWorkspace.frontmostApplication` gives bundle id, localized name, icon. Capture **at the moment `changeCount` flips** to minimize attribution races. Window title requires Accessibility and is best-effort.
- **Files:** read `public.file-url` / `NSFilenamesPboardType`; store URLs, not bytes. No special entitlement needed to *read* the pasteboard.

#### Windows - `AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE`

Fully event-driven. Create a **message-only window** (`HWND_MESSAGE`) on the capture thread, call `AddClipboardFormatListener(hwnd)`, pump messages; `WM_CLIPBOARDUPDATE` fires on every change with no polling. Reading formats requires `OpenClipboard`, which another process may hold; use bounded retry with exponential backoff (5 ms -> ~5 s) then drop the generation rather than deadlock.

Specifics:
- **Format enumeration:** loop `EnumClipboardFormats(0)` + `GetClipboardData` + `GlobalLock`. Map `CF_UNICODETEXT` -> `PlainUtf8` (keep original bytes too for fidelity), `CF_HDROP` -> `FileList`, `CF_DIB`/`CF_DIBV5` -> `Image`. Registered formats by name: `"HTML Format"` (CF_HTML, parse header for fragment offsets), `"Rich Text Format"`, `"PNG"`.
- **Sensitivity:** parse `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`, and `CanUploadToCloudClipboard` separately. The first blocks monitor processing, the second excludes local history, and the third excludes cloud upload without necessarily forbidding local capture. Treat `Clipboard Viewer Ignore` as a monitor exclusion. The policy layer, not the format reader, decides whether a marker means skip, local-only, or no-sync.
- **Self-write loop avoidance:** stamp our own writes with a sentinel registered format (`"vbuffOwnWrite"`) and skip events that carry it. `GetClipboardSequenceNumber()` alone can't distinguish source.
- **Source app:** start with `GetClipboardOwner`, then resolve its PID and executable. The owner can be `NULL`; only then use `GetForegroundWindow` as a lower-confidence heuristic. Store attribution confidence and never present the fallback as certain provenance.

#### Linux - X11 vs Wayland, detected at runtime

Detect session from `XDG_SESSION_TYPE` (fallback to `WAYLAND_DISPLAY` presence).

**X11 - XFIXES + CLIPBOARD selection ownership.** X11 has selections, not a buffer. Use **XFIXES** (`XFixesSelectSelectionInput` on `CLIPBOARD` with `SetSelectionOwnerNotifyMask`) for an event on every ownership change (a 250 ms `XGetSelectionOwner` poll is the ancient-server fallback). On notify, `XConvertSelection(CLIPBOARD, TARGETS)` to enumerate targets, then one convert per target into a property read via `XGetWindowProperty`; large transfers use the **INCR** protocol (must be handled or big images/files truncate). Specifics:
- **Owner-dies problem & persistence:** X11 selection data lives in the owning app and vanishes when it quits. A real manager must take `CLIPBOARD` ownership and re-serve `TARGETS`/data on `SelectionRequest` so content survives the source closing. This is why `arboard` cannot serve here.
- **PRIMARY selection:** optionally also watch `PRIMARY` for middle-click capture, gated behind a setting and debounced hard (it changes on every selection).
- **Source app:** `_NET_ACTIVE_WINDOW` -> `WM_CLASS` + `_NET_WM_NAME`; resolve icon from the `.desktop` file. Window-class ignore rules use `WM_CLASS`.
- **Password-manager hints:** no universal flag; detect known target names (e.g. `x-kde-passwordManagerHint`) and treat as concealed.

**Wayland - `ext-data-control-v1` first, deprecated `wlr-data-control` fallback, explicit unsupported mode.** Wayland forbids background clipboard snooping for ordinary clients. A compositor-advertised data-control manager receives selection events and offered MIME types without foreground focus. Prefer staging `ext-data-control-v1`; bind `wlr-data-control-unstable-v1` only when the newer protocol is absent and the legacy protocol is explicitly advertised. Specifics:
- **Capability probing** at startup: inspect the registry for the exact data-control protocol and version, then expose that result through `Capabilities`; do not infer support from `WAYLAND_DISPLAY` or the presence of `wl-paste`.
- **GNOME fallback:** if Mutter exposes neither supported path, degrade to **capture-on-summon** plus manual capture and communicate this clearly in UI rather than silently dropping copies. This is a genuine platform limitation, not a failed promise to hide.
- **Source app:** Wayland intentionally hides cross-client window info; `app-id`/title are generally **not retrievable** without compositor-specific protocols. Source-app tagging is best-effort and often `None`.
- **Sensitivity / primary:** honor the same password-manager-hint MIME list as X11; primary selection is exposed too, with the same noisy-debounce treatment.

### Format mapping (per-OS UTI / CF_* / MIME -> vbuff FormatKey)

A small central table keeps the rest of the app OS-agnostic. Unknown identifiers are preserved as `Custom(id)` byte-for-byte, never dropped.

| vbuff `FormatKey` | macOS UTI | Windows | X11/Wayland MIME |
|---|---|---|---|
| `PlainUtf8` | `public.utf8-plain-text` | `CF_UNICODETEXT` | `UTF8_STRING`, `text/plain;charset=utf-8` |
| `Html` | `public.html` | CF_HTML (`"HTML Format"`, parse header) | `text/html` |
| `Rtf` | `public.rtf` | `"Rich Text Format"` | `text/rtf`, `application/rtf` |
| `Image(Png)` | `public.png` | `"PNG"` / `CF_DIB`->encode | `image/png` |
| `Image(Bmp)` | `com.microsoft.bmp` | `CF_DIB`/`CF_DIBV5` | `image/bmp` |
| `FileList` | `public.file-url`, `NSFilenamesPboardType` | `CF_HDROP` | `text/uri-list` |
| `Custom(id)` | any other UTI | any registered name | any other MIME |

### Debounce & burst coalescing

Multi-format writers (browsers, Office, Electron) fire several notifications per logical copy. We coalesce:
- **Windows / Wayland (event-driven):** on first notify, start a ~40-80 ms timer; reset on any further notify; read flavors only when the window lapses, capturing the complete final set in one pass.
- **macOS (poll-based):** the poll interval already coalesces; if `changeCount` advances again mid-read, restart the read against the newest generation rather than emitting twice.
- **X11:** debounce `XFixesSelectionNotify`; ignore notifies whose owner is our own persistence window.

Coalescing also swallows the self-write notify from paste-back, combined with the sentinel-format check.

---

## Target storage, data model & search

vbuff's system of record. Every captured clip, snippet, tag, pin, sync record, and DB-resident setting that must survive a reboot lives here. Goals: fast enough for sub-frame type-to-filter over 100k+ items, durable enough to survive power loss mid-capture, private enough that a stolen disk yields nothing.

### TL;DR decisions

| Concern | Decision | Why |
|---|---|---|
| Engine | **SQLite** via **`rusqlite`** (bundled) | Single-file, transactional, FTS5 in-tree, zero server. |
| Search | **FTS5** external-content table over a normalized text column | Millisecond search over tens of thousands of rows; `prefix` index for true type-to-filter. |
| Encryption | **SQLCipher** (full-DB) by default, key in OS keychain | Encrypts pages incl. the FTS index and WAL; app-level field encryption is rejected (leaks the index). |
| Large blobs | **Out-of-row content-addressable files** for payloads > 256 KiB; small payloads inline | Keeps row scans and FTS fast; bounds page churn; dedups identical blobs. |
| Dedup | **BLAKE3 content hash** over the canonical bytes of every flavor set | Fast, collision-free in practice; drives move-to-top. |
| Concurrency | One writer connection (capture/eviction), a pool of read connections (UI), **WAL** | Capture never blocks the popup; readers see a consistent snapshot. |
| Migrations | `user_version` pragma + ordered embedded SQL steps | Deterministic, reviewable, testable; no proc-macro magic. |

Settings, hotkeys, exclusion lists, regex rules, and the per-app blacklist live in a human-editable TOML config file, **not** the DB. The DB is data; the config is policy. Only derived/cached settings the query layer needs at runtime are mirrored into `meta_kv`. No ORM (`diesel`/`sea-orm`): the schema is small, queries are hand-tuned for FTS and pagination, and an ORM obscures the exact SQL that determines latency.

### Data model

The core split is **item** (a logical clipboard event or saved object) vs. **flavor** (one MIME representation). Storing only the first flavor is explicitly disallowed.

```sql
CREATE TABLE item (
    id              INTEGER PRIMARY KEY,            -- rowid, monotonic
    kind            INTEGER NOT NULL,               -- 0 text,1 richtext,2 html,3 image,4 files,5 color,6 url,7 code,8 mixed
    content_hash    BLOB NOT NULL,                  -- BLAKE3 over canonical flavor-set bytes (dedup key)
    created_at      INTEGER NOT NULL,               -- first-capture epoch millis (UTC)
    updated_at      INTEGER NOT NULL,               -- bumped on re-copy (move to top)
    last_used_at    INTEGER,                        -- last paste (most-used sort / never-pasted filter)
    use_count       INTEGER NOT NULL DEFAULT 0,
    byte_size       INTEGER NOT NULL,
    source_app_id   INTEGER REFERENCES source_app(id),
    source_title    TEXT,
    pinned          INTEGER NOT NULL DEFAULT 0,     -- exempt from eviction
    favorite        INTEGER NOT NULL DEFAULT 0,
    permanent       INTEGER NOT NULL DEFAULT 0,     -- promoted to permanent store (never auto-prunes)
    secret          INTEGER NOT NULL DEFAULT 0,     -- masked, short retention, sync-excluded
    color_hex       TEXT,                           -- denormalized for color clips
    display_name    TEXT,                           -- custom alias replacing raw content
    note            TEXT,
    color_label     TEXT,
    collection_id   INTEGER REFERENCES collection(id),
    sort_index      REAL,                           -- manual drag-reorder (fractional indexing)
    local_only      INTEGER NOT NULL DEFAULT 0,     -- never leaves this device
    meta            TEXT                             -- JSON: per-kind extras (image WxH, file count, lang...)
);

CREATE TABLE flavor (
    id          INTEGER PRIMARY KEY,
    item_id     INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    mime        TEXT NOT NULL,                      -- canonical MIME
    os_format   TEXT,                               -- native id round-trip: UTI / CF_* / X11 target / Wayland mime
    storage     INTEGER NOT NULL,                   -- 0 inline, 1 external CAS file, 2 zstd-inline
    bytes       BLOB,                               -- present iff storage IN (0,2)
    blob_ref    BLOB,                               -- BLAKE3 of payload -> CAS path, iff storage = 1
    byte_size   INTEGER NOT NULL,
    is_primary  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_flavor_item ON flavor(item_id);
CREATE INDEX idx_flavor_blobref ON flavor(blob_ref) WHERE blob_ref IS NOT NULL;

CREATE TABLE source_app (
    id INTEGER PRIMARY KEY, bundle_id TEXT UNIQUE, name TEXT NOT NULL, icon_blob BLOB
);
CREATE TABLE tag (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE);
CREATE TABLE item_tag (
    item_id INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);
CREATE INDEX idx_item_tag_tag ON item_tag(tag_id);
CREATE TABLE collection (                            -- folders / tabs / pinboards
    id INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES collection(id) ON DELETE CASCADE,
    kind INTEGER NOT NULL,                            -- 0 folder,1 tab,2 pinboard
    name TEXT NOT NULL, locked INTEGER NOT NULL DEFAULT 0, sort_index REAL
);
CREATE TABLE snippet (
    id INTEGER PRIMARY KEY, folder_id INTEGER REFERENCES collection(id),
    name TEXT NOT NULL, abbreviation TEXT,
    body TEXT NOT NULL,                               -- tokens {date} {clipboard} {cursor} {field:Name}
    is_rich INTEGER NOT NULL DEFAULT 0, html_body TEXT, hotkey TEXT,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE device (
    id INTEGER PRIMARY KEY, device_uuid BLOB NOT NULL UNIQUE, name TEXT NOT NULL,
    platform TEXT NOT NULL, pubkey BLOB NOT NULL, last_seen_at INTEGER, trusted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE sync_log (                              -- replication + LWW conflict resolution
    item_id INTEGER NOT NULL REFERENCES item(id) ON DELETE CASCADE,
    device_id INTEGER REFERENCES device(id),
    lamport INTEGER NOT NULL,                        -- logical clock for deterministic last-writer-wins
    direction INTEGER NOT NULL, state INTEGER NOT NULL,
    PRIMARY KEY (item_id, device_id, direction)
);
CREATE TABLE meta_kv (key TEXT PRIMARY KEY, value TEXT);
```

`kind` (computed once at capture by the content classifier) drives UI/filters; the authoritative bytes live in `flavor`. A web copy yields ~3 flavors (`text/html`, `text/plain`, sometimes `image/png`) under one item; paste-time format selection picks among them. **Manual reorder** uses fractional indexing (`sort_index REAL`): drop between `a` and `b` -> `(a+b)/2`, with rare renormalization.

### Dedup strategy (content hashing)

Dedup operates on the **whole flavor set** because two copies are "the same" only if every paste-able byte is identical.

```rust
/// BLAKE3 over a deterministic serialization of all flavors (sorted by mime).
fn content_hash(flavors: &[CapturedFlavor]) -> [u8; 32] {
    let mut sorted: Vec<&CapturedFlavor> = flavors.iter().collect();
    sorted.sort_by(|a, b| a.mime.cmp(&b.mime));
    let mut hasher = blake3::Hasher::new();
    for f in sorted {
        hasher.update(f.mime.as_bytes());
        hasher.update(&(f.bytes.len() as u64).to_le_bytes()); // length-prefix avoids ambiguity
        hasher.update(&f.bytes);                                // raw bytes, never normalized
    }
    *hasher.finalize().as_bytes()
}
```

On capture, look up `content_hash`. Exists -> bump `updated_at`, item floats to top, no new row/blob. Absent -> insert. A **partial unique index** enforces the invariant for the rolling history while letting pinned/permanent duplicates coexist:

```sql
CREATE UNIQUE INDEX idx_item_hash ON item(content_hash) WHERE permanent = 0 AND pinned = 0;
```

### Retention & eviction

Multiple independent caps apply, all of which **skip pinned, favorite, and permanent items**:

| Cap | Trigger | Rule |
|---|---|---|
| Count cap (MVP) | after each insert | keep newest N where not protected; delete the rest oldest-first |
| Total size cap | after insert / on timer | delete oldest non-protected until `SUM(byte_size) <= budget` |
| Per-item size cap | at capture | reject or truncate before insert |
| Time expiry | timer (~5 min) | delete non-protected where `created_at < now - max_age` |
| Sensitive expiry | timer | shorter `max_age` for `secret=1` rows |
| Unlimited mode | config | count cap disabled; only size/time caps apply |

`ON DELETE CASCADE` removes flavors and FTS rows; CAS files are GC'd separately (refcount-by-query, with a ~60 s grace window so a just-written-but-uncommitted blob isn't reaped). "Clear entire history" = `DELETE FROM item WHERE NOT protected` + CAS GC + optional `VACUUM`.

### Large-blob handling (content-addressable store)

Payloads **≤ 256 KiB**: inline in `flavor.bytes` (zstd-compressed if it helps, `storage=2`). Payloads **> 256 KiB**: written to a **content-addressable store** at `blobs/<first2hex>/<full-blake3-hex>`; `flavor.blob_ref` holds the BLAKE3, `storage=1`. Identical blobs across items are stored once; row scans stay tiny; the writer streams to a temp file then `rename(2)` (atomic) before committing the row, so a crash never leaves a half-written referenced blob. Thumbnails are a parallel regenerable cache (`thumbs/<blake3>.webp`), never the source of truth. File/folder clips store the native list (`text/uri-list`/`CF_HDROP`/`NSFilenamesPboardType`) plus extracted metadata in `meta` JSON; the file bytes are **not** copied.

### Encryption at rest

**Decision: SQLCipher (full DB), key from OS keychain.** Per-row AEAD over plain SQLite leaves the FTS index, WAL, and metadata plaintext, revealing *what you searched and copied*; SQLCipher encrypts pages so the whole file (incl. index) is opaque, and FTS works unchanged. The out-of-row CAS blobs live outside DB pages, so each is sealed with **XChaCha20-Poly1305** (subkey via HKDF; random 24-byte nonce prepended). Master-password mode (v1) wraps the root key with `Argon2id(password)` so changing the password only re-wraps the key, never re-encrypts the DB; PIN is a weaker, rate-limited convenience unlock.

```rust
fn open_encrypted(path: &Path, key: &SecretKey) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "key", format!("x'{}'", hex::encode(key.0)))?; // raw 256-bit key
    conn.pragma_update(None, "cipher_memory_security", "ON")?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;  // WAL + NORMAL: crash-safe and fast
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "mmap_size", "268435456")?; // 256 MiB mmap for cold-start reads
    Ok(conn)
}
```

Wrong key surfaces as `SQLITE_NOTADB` on first read -> treat as "locked", prompt, never wipe. Auto-lock/idle closes connections and zeroizes the in-memory key. The `-wal`/`-shm` sidecars are also encrypted (checkpoint before backup). Cryptographically meaningful erase = deleting the key from the keychain, which renders any residual ciphertext permanently unreadable (documented honestly given SSD wear-leveling and CoW filesystems defeat in-place overwrite).

### Target migrations

Schema version in `PRAGMA user_version`; forward-only ordered SQL steps, each in one transaction, applied before the first read. Before migrating an existing DB, take a full-file backup (online backup API) so an aborted upgrade rolls back the whole file. Never auto-downgrade: if `user_version` exceeds the binary's max, refuse to open and tell the user to update.

### Indexing & query patterns for fast type-to-filter

Two tiers: substring (MVP default) via `instr`/`LIKE` for short histories and FTS for large; FTS5 indexed search (architected from day one) via an external-content table mirroring the item's searchable text projection.

```sql
CREATE VIRTUAL TABLE item_fts USING fts5(
    text, content='item_text', content_rowid='item_id',
    tokenize = "unicode61 remove_diacritics 2"  -- diacritic + case folding: 'cafe' finds 'café'
);
CREATE TABLE item_text (item_id INTEGER PRIMARY KEY REFERENCES item(id) ON DELETE CASCADE, text TEXT NOT NULL);
-- triggers keep item_fts in sync on insert/update/delete of item_text
```

Type-to-filter appends `*` to the last token for prefix matching (`rec` -> `rec*`). Conventional facet indices:

```sql
CREATE INDEX idx_item_updated      ON item(updated_at DESC);                 -- recency default
CREATE INDEX idx_item_kind_updated ON item(kind, updated_at DESC);          -- type chip + recency
CREATE INDEX idx_item_pinned       ON item(updated_at DESC) WHERE pinned=1; -- pinned section
CREATE INDEX idx_item_fav          ON item(updated_at DESC) WHERE favorite=1;
CREATE INDEX idx_item_source       ON item(source_app_id, updated_at DESC); -- filter by app
CREATE INDEX idx_item_collection   ON item(collection_id, updated_at DESC); -- collection-scoped
CREATE INDEX idx_item_created      ON item(created_at);                      -- date-range + expiry
```

`bm25()` provides relevance ranking; FTS5 `snippet()`/`highlight()` supply match offsets for live highlighting without re-scanning in Rust. **Pagination is keyset (seek) based**, not `OFFSET`, for deep scrolls:

```sql
SELECT ... FROM item
WHERE (updated_at, id) < (?cursor_ts, ?cursor_id)
ORDER BY updated_at DESC, id DESC LIMIT ?page;
```

Fuzzy (`recpt` -> `receipt`) and regex search run in Rust over an FTS-narrowed candidate set (`nucleo`/`fuzzy-matcher`, or the `regex` crate). Whole-table regex is opt-in, bounded by a row-scan budget with a visible "scanning…" state. The `item_text` projection is searchable surrogate text (e.g. `"PNG 1920x1080 from Safari screenshot"` for an image), so mixed-type histories search uniformly while canonical bytes stay untouched.

### Concurrency model (storage)

```
 capture thread (single writer) ──writes──► SQLite (WAL, one .db file)
   - hash, classify, upsert + evict                    │ reads (snapshot)
 UI thread (egui) ◄────────── r2d2 read pool ──────────┘   (writes go via channel to the writer)
```

One writer (all inserts/eviction/pin/tag/delete serialized through the daemon thread), pooled readers (WAL snapshots, never block on capture), WAL checkpointing on a timer/size threshold from the writer. The CLI/control socket sends commands to the writer rather than opening its own writer connection; a second process may open the DB for reads only.

### Capture-path data flow (where storage rules are enforced)

Every capture runs in **one transaction** (item + flavors + FTS + eviction) so an app crash or power loss never tears a clip across tables; WAL + `synchronous=NORMAL` makes the committed clip durable without the full-fsync cost.

```rust
fn on_clipboard_change(raw: RawClipboard, ctx: &CaptureCtx) -> CaptureOutcome {
    if ctx.paused || ctx.incognito { return Skipped; }
    if ctx.blacklist.contains(&raw.source_app) { return Skipped; }
    if raw.is_concealed() { return Skipped; }                 // ConcealedType / secure input / Wayland sensitive
    let flavors = decode_all_flavors(&raw);                    // every MIME, byte-for-byte
    if let Some(text) = primary_text(&flavors) {
        if ctx.cfg.skip_whitespace_only && text.trim().is_empty() { return Skipped; }
        if ctx.exclusion_regex.is_match(text) { return Skipped; }
    }
    if flavors.iter().map(|f| f.bytes.len()).sum::<usize>() > ctx.cfg.per_item_limit { /* reject/truncate */ }
    let kind = classify(&flavors);
    write_with(ctx.writer, move |tx| {
        let up = upsert_capture(tx, &Capture { flavors, kind, now: ctx.now })?;
        update_search_projection(tx, &up)?;
        evict(tx, &ctx.cfg.retention)?;
        Ok(())
    })
}
```

### Per-OS storage differences

The schema is identical across platforms; what varies is the bytes that arrive, how concealment is signaled, and where the file lives.

| Concern | macOS | Windows | Linux (X11) | Linux (Wayland) |
|---|---|---|---|---|
| Default DB path | `~/Library/Application Support/vbuff/vbuff.db` | `%APPDATA%\vbuff\vbuff.db` | `$XDG_DATA_HOME/vbuff/vbuff.db` | same as X11 |
| `flavor.os_format` | UTIs (`public.utf8-plain-text`, `public.html`, `public.png`, `public.file-url`) | `CF_UNICODETEXT`, `CF_HTML`, `CF_DIB`/`PNG`, `CF_HDROP` | X11 targets (`UTF8_STRING`, `text/html`, `image/png`, `text/uri-list`) | Wayland MIME |
| Concealment -> `secret`/skip | `org.nspasteboard.ConcealedType`, Secure Event Input | monitor exclusion and local-history exclusion; cloud-upload exclusion remains a distinct no-sync input | password-manager hint atoms (best-effort) | sensitive flag where supported |
| File clips | file URLs / `NSFilenamesPboardType` | `CF_HDROP` paths | `text/uri-list` | `text/uri-list` |
| Key store | Keychain | Credential Manager / DPAPI | Secret Service (GNOME Keyring) | Secret Service / KWallet; encrypted-file fallback |
| Persistence quirk | survives app exit | survives app exit | **selection owner-held**: must take ownership to persist | data-control support needed; otherwise only foreground/manual capture is claimed |

Path resolution uses `directories::ProjectDirs`; users can override the location (validated writable, on a filesystem with `rename(2)` atomicity; warn on network/FUSE mounts).

---

## Target security, privacy & permissions

A clipboard manager sees every password, OTP, API key, and private message that transits the clipboard. The target threat model is the spine of the architecture: **vbuff defaults to local-only, encrypts at rest, and treats "do not capture" as a first-class, fail-closed code path that runs before anything touches disk.** The current SQLite store is not encrypted, and generic `arboard` cannot supply the native privacy evidence required to claim the rest of that sentence as shipped behavior.

### Threat model

| Adversary | Capability | vbuff's defense | Out of scope |
|---|---|---|---|
| Stolen/lost disk, backup leak, stray file copy | Offline read of `vbuff.db` and blob dir | AEAD encryption at rest; key in OS keychain, never beside the DB | - |
| Another local unprivileged user/process | Reads our files with their own privileges | File-mode 0600, per-user keychain ACL; key not derivable from DB alone | A root/admin attacker |
| Shoulder-surfer / unattended unlocked session | Opens popup, scrolls history | Master-password/PIN lock, auto-lock on idle, masked sensitive rows | - |
| Sensitive content entering history | Password-manager copy, secure field, OTP | Concealed-flag honoring, secure-field detection, exclusion lists, regex/secret rules, incognito | A writer that sets no hint in an app we can't identify |
| Memory scraping of our own process | Reads our RAM | `zeroize` of keys/plaintext, best-effort `mlock` | A debugger attached to our PID; kernel attacker |
| Network exfiltration | Sniffs/MITMs sync traffic | E2E (Noise) for LAN sync; zero network by default | - |

Two hard rules: **(1) Fail closed** - every uncertainty in the capture decision resolves to *skip*. **(2) The key never lives next to the ciphertext** - it lives in the OS secret store.

### The capture-decision gate (the most important code path)

Before any clip is hashed or read into long-lived memory, it passes one ordered gauntlet: cheap certain rejections first, expensive content scanning last, everything fail-closed. Every "Security & privacy" / "Capture exclusion" feature maps to exactly one `SkipReason`, keeping the policy auditable.

```rust
pub enum CaptureDecision { Capture { sensitivity: Sensitivity }, Skip(SkipReason) }
pub enum Sensitivity { Normal, Sensitive }  // Sensitive: shorter retention, masked, never synced
pub enum SkipReason {
    MonitoringPaused, Incognito, ConcealedFlag, SecureInputActive,
    ExcludedApp(String), PrivateBrowsing, PatternMatch(String),
    WhitespaceOnly, OverSizeLimit { bytes: usize, limit: usize },
    SourceUnknownFailClosed,
}

fn evaluate(&self, ctx: &CaptureContext, content: &ClipPreview) -> CaptureDecision {
    use SkipReason::*;
    // 0. Global kill switches.
    if ctx.runtime.monitoring_paused { return Skip(MonitoringPaused); }
    if ctx.runtime.incognito         { return Skip(Incognito); }
    // 1. OS sensitivity hints (authoritative).
    if ctx.os_hints.concealed || ctx.os_hints.transient { return Skip(ConcealedFlag); }
    if ctx.secure_input_active  { return Skip(SecureInputActive); }
    // 2. Source identity; fail closed if unknown and required.
    match &ctx.source {
        Some(app) => {
            if ctx.settings.is_app_excluded(app) { return Skip(ExcludedApp(app.identifier())); }
            if ctx.settings.skip_private_browsing && app.is_private_browser_window() { return Skip(PrivateBrowsing); }
        }
        None if ctx.settings.require_known_source => return Skip(SourceUnknownFailClosed),
        None => {}
    }
    // 3. Size cap (before reading the full body).
    if let Some(limit) = ctx.settings.per_item_byte_limit {
        if content.declared_len > limit { return Skip(OverSizeLimit { bytes: content.declared_len, limit }); }
    }
    // 4. Whitespace/empty.
    if ctx.settings.skip_whitespace_only && content.is_text_and_blank() { return Skip(WhitespaceOnly); }
    // 5. Content scanning (only now do we look at bytes).
    if let Some(text) = content.text_for_scan() {
        if let Some(rule) = ctx.settings.first_matching_pattern(text) { return Skip(PatternMatch(rule.name.clone())); }
        let sensitivity = if self.secret_detectors.looks_secret(text) { Sensitivity::Sensitive } else { Sensitivity::Normal };
        return CaptureDecision::Capture { sensitivity };
    }
    CaptureDecision::Capture { sensitivity: Sensitivity::Normal }
}
```

### Honoring OS concealed / sensitive markers

The highest-leverage privacy win: well-behaved password managers already tell us not to store.

| OS | Marker | How we read it |
|---|---|---|
| macOS | `org.nspasteboard.ConcealedType`, `...TransientType`, `...AutoGeneratedType` | Enumerate `NSPasteboard.types`; presence -> skip/transient |
| Windows | `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`, `CanUploadToCloudClipboard` | Enumerate after `WM_CLIPBOARDUPDATE`; preserve three distinct decisions: monitor exclusion, local-history exclusion, and cloud-upload exclusion |
| Linux X11/Wayland | `x-kde-passwordManagerHint` / password-manager hint targets | Present in TARGETS / wlr offers -> skip |

These are conventions, not enforced contracts. Defense in depth: hints are gate #1; secure-field detection, app exclusion, and content/secret rules catch what hints miss. Documented as best-effort.

### Secure-field & private-context detection (graceful degradation)

| Capability | macOS | Windows | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| Foreground/source app identity | `NSWorkspace.frontmostApplication` | `GetClipboardOwner`->exe; foreground fallback with lower confidence | `_NET_ACTIVE_WINDOW`->`WM_CLASS` | **Not available** to ordinary clients |
| Secure input active | `IsSecureEventInputEnabled()` (global) | rely on concealed hint | rely on hints | rely on hints |
| Private-browsing window | bundle id + title heuristics | title/class heuristics | `WM_CLASS` + title | app-id only, limited |

On macOS, secure input active -> both skip capture and suppress paste-back keystroke synthesis, with a UI explanation. **Wayland is the hard case:** a normal client cannot learn the foreground app or secure-field focus; this is the OS protecting the user. We respect it: app-exclusion and private-browser skip simply cannot function, so the settings panel shows a per-platform capability badge ("App exclusion: unavailable on this Wayland session, content-pattern rules still apply") rather than lulling the user into false safety.

### Key management & OS keychain

A single random 256-bit root DEK, stored in the OS secret store, with purpose-bound subkeys via HKDF-SHA256 (`sqlcipher_key`, `blob_key`). The `SecretStore` trait abstracts the backend:

| OS | Backend | Notes & failure modes |
|---|---|---|
| macOS | Keychain | Per-user, ACL-scoped to signed code identity; hardened-runtime build required |
| Windows | Credential Manager (+ **DPAPI** fallback) | Credential Manager needs an interactive session; DPAPI covers headless-service principals |
| Linux | Secret Service (GNOME Keyring / KWallet) over D-Bus | Needs the daemon unlocked; **no daemon** (headless/SSH) -> encrypted key file with Argon2id-derived KEK. Never silently drop to a plaintext key file |

The encrypted-file fallback is itself a `SecretStore` impl; the UI shows which backend is active and warns on the fallback.

### Lock, auto-clear, incognito, secure wipe

- **Incognito** flips one atomic flag read at gate #0; nothing touches disk, instant. "Pause" is the same mechanism with persistent-across-session UX and a tray indicator.
- **Lock** (manual / idle-timeout / OS screen-lock signal: `com.apple.screenIsLocked`, `WTS_SESSION_LOCK`, logind `Lock`) zeroizes the in-memory DEK and cached plaintext, drops the SQLCipher connection, requires unlock.
- **Retention** runs two timers: ordinary clips honor the configured age; `Sensitive` clips get a separate shorter expiry. Pins/favorites are exempt from caps, but an explicit "wipe all incl. pinned" panic option exists.
- **Secure wipe** is a real delete (`secure_delete=ON; VACUUM`) plus best-effort blob overwrite; the cryptographically meaningful erase is deleting the DEK.

### Required OS permissions & degradation

| Permission | OS | Needed for | If denied |
|---|---|---|---|
| Accessibility (`AXIsProcessTrusted`) | macOS | Restore focus, synthesize Cmd+V | Copy-only mode: clip goes to clipboard, user pastes manually; onboarding deep-links to the pane |
| Input Monitoring | macOS | Only if using CGEventTap for hotkey | Prefer Carbon `RegisterEventHotKey` (no Input Monitoring) |
| GlobalShortcuts portal | Wayland | Global hotkey | If absent, document per-compositor setup; offer CLI/tray invocation |
| Secret Service daemon | Linux | Keychain | Encrypted-file fallback |

The principle throughout: **a missing permission degrades a feature, it never blocks the app or silently fails.**

### Two subtle traps

1. **Self-write suppression.** When vbuff writes the clipboard (paste or restore-prior-contents), our monitor sees the change. Without suppression we re-capture our own paste (or a restored *sensitive* clip). Tag every vbuff write (content hash + short-TTL "we just wrote this" flag + sentinel format) and gate #0 ignores it.
2. **macOS change-count race.** Between detecting `changeCount` increment and reading, another app can overwrite. Read all flavors in one pass and re-check `changeCount`; if it moved, re-read or skip rather than store a torn mix.

---

## Target crate dependency table

| Crate | Purpose | Notes / trade-offs |
|---|---|---|
| `egui` + `eframe` | GUI toolkit (popup + settings viewports) | Immediate-mode -> free row virtualization (`ScrollArea::show_rows`), trivial custom rows. Weak BiDi/complex-shaping; mitigate with a `cosmic-text` galley for clip content. Chosen over `iced` (which has better complex-script but manual virtualization and worse hot-path fit). |
| `cosmic-text` | Complex-script / BiDi shaping for clip *content* text | Galley layer inside egui so CJK/Indic/Arabic/emoji render correctly; egui chrome stays egui. |
| `accesskit` | Accessibility tree (UIA / AT-SPI / NSAccessibility) | Integrated with egui; covers screen-reader requirement. Newer, less battle-tested. |
| `rusqlite` (features `bundled-sqlcipher-vendored-openssl`, `blob`, `functions`, `serde_json`, `backup`, `limits`) | Embedded store driver | Thin, exposes FTS5, incremental BLOB I/O, custom functions, online backup. `bundled` pins SQLite/SQLCipher identically across OSes. Vendored OpenSSL increases build time/binary size. Chosen over `sqlx` (async/compile-checked queries add no value to a single-process synchronous embedded store). |
| `r2d2` + `r2d2_sqlite` | Read-connection pool for the UI | Simple pooling; writes go through the single writer actor, not the pool. `deadpool-sqlite` is the async alternative if the store ever goes async. |
| `blake3` | Content hash (dedup) + CAS file naming | ~GB/s, 256-bit; far faster than SHA-256. Hash format is pinned by a golden-vector test (changing it silently breaks dedup). |
| `zstd` | Compress large text/HTML before CAS/inline write | Images stored as-is (already compressed). |
| `image` | Decode bitmaps, sniff format, generate thumbnails | Runs on the capture thread, never the UI thread. |
| `infer` | Magic-byte content sniffing | Confirms declared MIME vs actual bytes; cheap defense against mislabeled clips. |
| `arboard` | Degraded clipboard fallback and early smoke tests | It is the current text-or-image poller, but the target architecture removes it from the primary capture path because it has no events, all-flavor read, concealed hints, source, or generation proof. |
| `objc2`, `objc2-app-kit`, `objc2-foundation` | macOS clipboard/workspace/run-app bindings | Modern, maintained successor to `cocoa`/`objc`; typed `NSPasteboard`/`NSWorkspace`. Avoid unmaintained `cocoa`. |
| `windows` (windows-rs) | Win32 clipboard listener, `SendInput`, foreground window, DPAPI | First-party bindings covering `AddClipboardFormatListener`, `GetClipboardData`, etc. |
| `x11rb` (with `xfixes`) | X11 selections + XFIXES selection events | Pure-Rust XCB bindings; safer than `x11-dl`. Needed for INCR transfer and selection ownership. |
| `wayland-client` + generated protocol bindings | `ext-data-control-v1` client with legacy `wlr-data-control` compatibility | Registry-probed, event-driven capture where supported. `wl-clipboard` is useful for bring-up/manual operations but is not evidence that background monitoring works. |
| `global-hotkey` | Hotkey registration (macOS/Windows/X11) | From the Tauri ecosystem; does **not** cover Wayland. |
| `ashpd` | Wayland GlobalShortcuts via xdg-desktop-portal | Required because Wayland forbids raw grabs; two code paths on Linux is unavoidable. |
| `tray-icon` | Tray / menu-bar (NSStatusItem / Shell_NotifyIcon / SNI) | Most-maintained option; SNI path needs a StatusNotifier host, hence the XEmbed fallback caveat. |
| `enigo` | Baseline keystroke injection helper | Used as a baseline only; focus-restore ordering and terminal-aware combos call OS APIs (`CGEvent`/`SendInput`/`XTEST`) directly for control. Wayland uses `wtype`/`ydotool`/virtual-keyboard. |
| `keyring` (v3) | OS secret store (Keychain / Cred Manager / Secret Service) | One API over three stores; Secret Service needs a running daemon -> encrypted-file fallback; pair with DPAPI for headless Windows. |
| `chacha20poly1305` | AEAD for out-of-row CAS blobs | XChaCha20-Poly1305: 192-bit nonce makes random nonces safe; no AES-NI dependence. `aes-gcm` is the hardware-AES alternative but its 96-bit nonce is easier to misuse. |
| `argon2` | KDF for master-password mode | Argon2id, memory-hard; ~250 ms target. `pbkdf2` only as a FIPS fallback. |
| `hkdf` + `sha2` | Subkey derivation from the root DEK | Purpose-bound subkeys (`sqlcipher`, `blob`). |
| `zeroize` (+ `secrecy`) | Wipe keys/plaintext on drop | Doesn't defend against swap; pair with `mlock`. |
| `region` | Best-effort `mlock`/`VirtualLock` on key pages | Silently fails without privilege (RLIMIT_MEMLOCK); treat success as a bonus. |
| `subtle` | Constant-time compare | PIN/password/pairing-code verification without timing leaks. |
| `regex` | Exclusion rules + built-in secret detectors | Anchored, size-bounded; short-circuit to bound per-clip CPU. |
| `nucleo` (or `fuzzy-matcher`) | In-memory fuzzy ranking over the FTS candidate set | FTS narrows to a few hundred rows; fuzzy ranks them. |
| `directories` | Per-OS default data paths | Application Support / AppData / XDG. |
| `serde` / `serde_json` | IPC framing, JSON export/import, JSON columns, custom-MIME maps | Shared down to `vbuff-types`. |
| `time` (or `jiff`) | UTC epoch-millis timestamps | Store as INTEGER; locale formatting in the UI layer only. |
| `tokio` | Daemon async runtime (IPC server, mDNS, sync sockets, timers) | The GUI runs on eframe's own loop; capture polling is a dedicated thread, not a tokio task, to isolate it from runtime stalls. |
| `crossbeam` | Channels between watcher/store/daemon threads | Low-latency mpmc/mpsc for the hot capture path. |
| `proptest` | Property tests (byte-fidelity, eviction invariants, fail-closed) | The core's primary regression net. |
| `criterion` | Benchmarks (type-to-filter latency, insert/evict throughput, cold-start) | Targets sub-frame search at 100k items. |

Crate-maturity claims (`tray-icon` SNI behavior, `global-hotkey` Wayland gap, exact `keyring` v3 surface, current SQLCipher PRAGMA defaults, `objc2-app-kit`/`windows-rs` signatures) are from general knowledge and **must be verified against pinned versions before the stack is locked.**

---

## Target failure modes and degradation (consolidated)

| Failure | Detection | Behavior |
|---|---|---|
| Clipboard owner holds/locks selection or hangs | bounded retry + timeout on `read_all` / `OpenClipboard` / X11 convert | back off (5 ms -> ~5 s), skip this generation, never block the watcher loop |
| Multi-format writer fires N change events | debounce window after first event | collapse to one capture |
| X11 INCR large-transfer truncation | INCR chunk reader | accumulate chunks; cap at per-item limit and mark truncated |
| Owner app exits before we read (X11/Wayland) | selection notify | read immediately on notify; for survival, take selection ownership (X11 persistence role) |
| Wayland compositor advertises no supported data-control protocol | capability probe at startup | degrade to capture-on-summon + manual hotkey; surface a persistent capability state and one-time explainer |
| Self-write feedback loop | sentinel format + own-ownership tracking + short-TTL flag | coalesce window swallows the echo; gate #0 ignores it |
| macOS Accessibility not granted | `AXIsProcessTrusted()` false | popup works copy-only; banner guides user; paste-back disabled until granted |
| macOS Secure Input active | `IsSecureEventInputEnabled()` | pause capture + keystroke synthesis, inform user |
| macOS change-count race | re-check `changeCount` after read | re-read or skip rather than store a torn mix |
| Wayland: no key injection | `PasteCaps` reports unavailable | set-and-let-user-paste instead of auto-paste |
| Source-app attribution unavailable | mostly Wayland | `SourceApp::default()`; UI shows "Unknown source"; identity-dependent features badged unavailable |
| Hotkey already taken | `is_available()` at bind time | flag conflict in settings, refuse bind |
| SQLite power-loss mid-write | WAL + `synchronous=NORMAL` | last committed capture survives; DB never corrupts |
| DB corruption (bad shutdown/disk) | `PRAGMA integrity_check` at startup / `SQLITE_CORRUPT` | attempt WAL recovery; else rename to `vbuff.corrupt.<ts>.db`, start fresh, non-destructive notice |
| Wrong encryption key | `SQLITE_NOTADB` on first read | treat as locked, prompt; never wipe |
| Disk full | `SQLITE_FULL` on commit | abort the capture txn (no torn write), pause capture, notify; eviction is the recovery lever |
| CAS write ok but commit fails | orphan blob | GC grace-window sweep reclaims it; no dangling reference (row never committed) |
| Schema version newer than binary | `user_version > MAX` | refuse to open; instruct user to update |
| Two instances launched | IPC bind fails | forward intent to live daemon, exit |
| Stale socket after crash | bind error + liveness probe | unlink, rebind once |
| Secret store unavailable (headless Linux) | Secret Service connect fails | encrypted-file key store, warn |

---

## Target testing strategy (consolidated)

- **`vbuff-core` is the test crown jewel.** Because it depends only on traits, dedup/eviction/retention/redaction/search-ranking are unit- and property-tested (`proptest`) against an in-memory `FakeStore`/`FakeClipboard`. Invariants: pinned items never evicted under any cap; byte-for-byte content survives store->load round-trip (invalid UTF-8, NUL bytes, CRLF, trailing newlines, RTL, emoji, 4-byte codepoints); identical content never produces two rows.
- **Capture gate** is table-driven: one case per `SkipReason`, plus `Capture+Sensitive`/`Capture+Normal`. A `proptest` invariant asserts that paused/incognito/concealed/secure-input/source-unknown-and-required *always* yields `Skip` (a `Capture` here is a release blocker).
- **Secret detectors:** corpus tests (Luhn-valid/invalid cards, JWTs, PEM keys, AWS keys -> Sensitive; UUIDs/hashes/prose -> Normal), tracked precision/recall, fail CI on regression.
- **Crypto:** `seal_blob`/`open_blob` round-trip; one-byte tamper of ciphertext/nonce/tag -> AEAD error; wrong key -> failure not garbage. Master-password wrap/unwrap; wrong password fails; password change preserves DB readability. The **canary-grep at-rest test** writes `CANARY_SECRET`, closes the DB, then scans raw DB/WAL/SHM/CAS/temp/log artifacts for zero hits. Run it on the active Windows release lane and repeat it independently for every later promoted native adapter.
- **Backend trait conformance suite:** a shared `#[test]` battery parameterized over any `ClipboardBackend`/`PasteBackend` impl, plus a `MockBackend` emitting scripted `CaptureEvent`s to drive daemon policy with zero OS deps. Run per-OS in a CI matrix (macOS, Windows, Ubuntu-X11 via `Xvfb`, Ubuntu-Wayland via headless `sway`).
- **Format mapping:** table-driven round-trip of UTI/CF_*/MIME <-> `FormatKey`, the CF_HTML header parser, and unknown->`Custom` preservation.
- **Store:** migrations forward-apply on checked-in fixture DBs from each prior schema version; WAL crash-recovery by SIGKILL mid-transaction then assert last committed clip present + `integrity_check`; disk-full via a small loopback/quota FS; FTS5 latency benchmarked at 50k rows (target < 8 ms for the SQL+map step). FTS correctness: diacritic folding, case-insensitivity, prefix matching, CJK tokenization, `highlight()` offset alignment.
- **Concurrency/contention:** writer thread + N reader threads paging/searching; a stress harness writing the clipboard from N threads/processes; assert no `SQLITE_BUSY` escapes, no hang/deadlock, bounded retries, no duplicate rows beyond dedup, snapshot consistency under WAL.
- **IPC contract:** serialize/deserialize every `ClientIntent`/`Response`; single-instance handoff tested by spawning two processes.
- **GUI:** filter/highlight/selection logic extracted into pure functions tested headless; egui rendering smoke-tested via `egui_kittest`; permission degradation injects `PasteCapability::ClipboardOnly` and asserts copy-only fallback.
- **Manual matrix** for irreducibly OS-specific bits: real Accessibility prompt, real Secure Input, real Wayland compositors (GNOME vs KDE vs sway).

---

## Target implementation roadmap

Phased to deliver a usable, private, single-machine clipboard manager first, then breadth, then networked and team features. Each phase maps to a milestone with explicit exit criteria.

### Phase 0 - Foundations (pre-MVP scaffolding)

- Cargo workspace + crate skeleton; `vbuff-types`, the four backend traits, `cfg`/runtime selection skeleton with mock backends.
- `vbuff-store` schema v1, migrations harness, WAL + SQLCipher open path, content-hash + golden-vector test.
- `vbuff-core` engine with dedup/eviction/capture-gate against fakes; the full `proptest` byte-fidelity + fail-closed suites.
- **Exit:** core logic is fully testable headless on any host; `vbuff` runs as a no-op daemon with single-instance guard.

### Phase 1 - Full-history recall (first implementation slice)

Milestone: **every stored clip is reachable without loading the history into the egui frame.**

- Add one `HistoryQuery { query, facets, cursor, limit } -> HistoryPage` boundary that returns row summaries, not hydrated payloads.
- Query SQLite/FTS off the egui thread, cancel stale generations, keyset-page results, and merge the bounded process-only lane into the projection without persisting it.
- Hydrate only the selected item for preview or delivery. Keep the current design tokens, stable row geometry, keyboard navigation, and typed health/evidence states.
- Benchmark the actual popup path against 100,000 rows; include a regression case that retrieves an item older than the first 1,000.
- **Exit:** p95 first results <= 50 ms, warm interactive p99 <= 16 ms, no full-table hydration, and ten idle repaint cycles cause zero projection rebuilds.

### Phase 2 - Verifiable Windows 11 alpha

Milestone: **the copy -> store -> find -> deliver loop is encrypted and evidence-backed on one declared Windows 11 x86-64 session class.**

- Replace the flattened clip representation with `ClipboardSnapshot -> ClipboardItem[] -> Flavor[]`, preserving order, native format IDs, realization state, generation evidence, and canonical source bytes.
- Implement `WM_CLIPBOARDUPDATE`, sequence-gap accounting, owner evidence, bounded format enumeration, exact writes, and separate monitor/history/cloud policy markers. The generic `arboard` path remains an explicitly degraded fallback, never native proof.
- Wire SQLCipher to Credential Manager/DPAPI key lifecycle and scan DB/WAL/SHM/CAS/temp/log artifacts for plaintext canaries.
- Capture the target before opening the picker, reconfirm it immediately before `SendInput`, and expose independent `Staged`, `TargetConfirmed`, `InjectionSent`, and integration-only `ApplicationAcknowledged` states. Elevated, changed, or otherwise unproven targets are copy-only.
- Keep the initial app/format matrix narrow: representative browser, IDE, terminal, and Office routes for text and CF_HTML. RDP, elevated targets, images, files, custom formats, and sensitive paste remain unsupported until their own rows pass.
- **Exit:** 10,000 observed edges are stored exactly once or explicitly gap-accounted; each supported format passes 1,000 canonical round trips; zero wrong-target injections; zero durable canary hits; and 14 days of sole-manager dogfood produce no silent observed-state loss.

### Phase 3 - Fidelity and contextual-recall beta

Milestone: **publish compatibility evidence that compounds, then prove contextual recall beats text-only search.**

- Turn `format-fidelity-v1` into a public, versioned app-pair corpus and generated support matrix. A supported row permits no silent downgrade; unsupported routes degrade visibly.
- Add encrypted source/time/session metadata, deterministic facets, surrounding-copy navigation, and a labeled ranking benchmark. Contextual ranking ships only with at least a 20% accepted top-three lift across 200 labeled retrievals.
- Add dry-run importers for Maccy, CopyQ, Ditto, and PasteBar through safe exports or snapshots, with a machine-readable loss manifest.
- Complete Windows packaging/signing, bootstrap recovery, autostart verification, keyboard-only workflows, text scaling, and screen-reader evidence. AccessKit roles and image goldens are foundations, not assistive-technology proof.
- **Exit:** the Windows beta passes its engineering gates and a demand gate: 20 target users, 12 activating without docs, 8 using vbuff four days per week after 30 days, and 5 willing to pay at least USD 25.

### Phase 4 - Gated expansion

No expansion item is implied by the beta roadmap. Promote work only after the Phase 3 demand gate:

- A second real native adapter must pass the same conformance and app-pair evidence before any cross-platform parity claim. Choose the OS from observed demand, not abstraction convenience.
- Extract daemon/IPC only when a second live client needs it; protocol contracts alone are not shipped integration.
- If device transfer is demanded, prove one explicit authenticated, TTL-bound, non-sensitive text handoff with replay protection and a signed receipt before ambient replication, CRDT, relay, or cloud-drive work.
- Keep MCP, plugin execution, remote automation, OCR, generic AI actions, ambient sync, mobile peers, and team collaboration frozen until a paid native beta supplies a narrower use case and an owner for its security boundary.
- **Exit:** each promoted capability has its own measurable gate. There is deliberately no pre-committed "all platforms plus sync" completion claim.

---

## Key risks & mitigations

| Risk | Likelihood / Impact | Mitigation |
|---|---|---|
| **A Wayland compositor advertises no supported background data-control protocol** (currently including important Mutter configurations) | High / High - a large Linux user segment | Probe `ext-data-control-v1` and legacy `wlr-data-control` exactly; degrade to capture-on-summon + manual hotkey; show an honest capability state. Track compositor and portal progress without claiming generic Wayland parity. |
| **Wayland hides foreground-app identity** -> per-app exclusion and private-browser skip can't function | High / Medium | Fail-closed where required is configurable; show per-platform capability badges so users aren't lulled into false safety; content/regex/secret rules still work and are surfaced as the active protection. |
| **egui's weak BiDi/complex-script shaping** blocks RTL/CJK/Indic users at launch | Medium / High for affected locales | Commit to a `cosmic-text` galley layer for clip *content* from v1; keep iced as the documented escape hatch if fidelity remains insufficient. |
| **Crate-maturity assumptions wrong** (`tray-icon` SNI, `global-hotkey` Wayland gap, `keyring` v3, SQLCipher PRAGMA defaults, objc2/windows-rs signatures) | Medium / Medium | Verify every platform crate against pinned versions in a Phase-0 spike before locking the stack; the trait boundaries make swapping an impl cheap. |
| **Encryption silently not engaged** (wrong PRAGMA, fallback to plain SQLite, blob written before sealing) | Low / Critical | Whole-artifact canary scan on Windows and each independently promoted native adapter; pin cipher params explicitly in a config record; treat any hit as a release blocker. |
| **Sensitive content leaks** because a source app sets no concealed hint | Medium / High | Defense in depth: hints + secure-field detection + app exclusion + built-in secret detectors + regex rules + incognito; sensitive items get masked display, shorter retention, and sync exclusion. |
| **Clipboard-owner contention / hangs** stall the watcher | Medium / Medium | Bounded retry with exponential backoff and timeouts; drop the generation rather than block; never hold the watcher loop. |
| **Self-write feedback loop** re-captures our own paste (or a restored sensitive clip) | High if unhandled / Medium | Sentinel format + own-ownership tracking + short-TTL content-hash flag; debounce window swallows the echo; gate #0 ignores tagged writes. |
| **DB corruption / power loss** | Low / High | WAL + `synchronous=NORMAL`, transactional-per-capture, `integrity_check` at startup, quarantine-and-restart on unrecoverable corruption, online-backup before migrations. |
| **UI hot-path contends with the writer under huge histories** | Low / Medium | One-writer + pooled-reader WAL design; if contention appears, promote the read path to an explicit snapshot/MVCC view. |
| **Sync correctness** (concurrent edits, duplicate arrivals across devices) | Medium / Medium (v2) | Lamport logical clock with deterministic last-writer-wins fallback; per-item sync state in `sync_log`; sync built only after the single-machine store is proven stable. |
| **Cross-platform binary size / packaging** (dual-compiled Linux X11+Wayland, vendored OpenSSL) | Low / Low | Accept for parity; revisit X11/Wayland feature flags if distribution size becomes a real constraint. |
| **Scope sprawl** across the very large feature set | High / Medium | Strict MVP -> v1 -> v2 phasing with per-milestone exit criteria; networked/team features deliberately last so the private single-machine core ships and stabilizes first. |

---

## How vbuff avoids competitors' mistakes

This section maps the most damaging and most frequent failures observed across existing clipboard managers (Ditto, CopyQ, Maccy, Paste, Pastebot, GPaste, Klipper, Win+V, cliphist, clipman, Diodon, Flycut, Clipy and others) to the concrete vbuff design decision that prevents each one, the crate that owns it, and the safeguard or test that enforces it. The mapping is grounded in the canonical spine pitfalls and the categorized anti-patterns in `docs/mistakes-top-500.md`.

Note on sourcing: this prompt referenced a pitfalls JSON (with `pitfallsExec` / `topMistakesSummary` keys), but no such file or keys exist in the repository (the path resolved to "undefined"). The specifics below are drawn from the inline `<spine>` pitfalls and from `docs/mistakes-top-500.md`.

| # | Competitor mistake (who) | vbuff design decision | Crate(s) | Safeguard / enforcing test |
|---|---|---|---|---|
| 1 | Fixed-interval polling can miss fast copies and burns CPU/battery | Subscribe to native change events: `AddClipboardFormatListener`/`WM_CLIPBOARDUPDATE` (Windows), `ext-data-control-v1` with legacy `wlr-data-control` fallback plus X11 `XFIXES SelectionNotify` (Linux); poll only `changeCount` on macOS with adaptive idle backoff | `vbuff-platform` | Idle CPU near 0% over a multi-day session; event-driven backends assert N generated edges -> N entries, while polling backends assert every observed state and every detectable counter gap is accounted for |
| 2 | macOS apps re-read full pasteboard every tick, stalling the source app (Maccy) | On each tick read ONLY the integer `changeCount`; read actual content exactly once, only when `changeCount` incremented | `vbuff-platform` (macOS backend) | Source-app re-render not triggered on idle ticks; content read counted == change-edge count in test |
| 3 | Missing copies during rapid successive copying | Capture synchronously inside event-driven handlers, enqueue, and persist on a separate worker; on polling APIs, compare sequence counters and record gaps that cannot be reconstructed | `vbuff-platform` -> `vbuff-store` async write queue | Capture-observability metric: exact N-edge recovery on event-driven backends; no silent gap, false recovery, or duplicate claim on polling backends |
| 4 | Clip lost when source window closes on Wayland/X11 - the single most-reported Linux frustration (X11/Wayland core, GPaste, Klipper, Diodon, Clipman, CopyQ) | Eagerly materialize all offered MIME types into vbuff's own store the instant a selection event fires; on X11 take ownership / cache bytes immediately. vbuff IS the persistence helper (built-in wl-clip-persist behavior) | `vbuff-platform` (Linux backend) → `vbuff-store` | Test: copy, close source app on Wayland and X11, assert clip still pasteable |
| 5 | Relying on a compositor protocol GNOME Mutter refuses to implement -> empty history on GNOME Wayland (cliphist, clipman, wl-clipboard) | Detect compositor capability at startup; fall back gracefully (portal / shell-extension bridge / XWayland-scoped) and surface what is and isn't captured. Never fake capability (non-goal) | `vbuff-platform`, surfaced by `vbuff-daemon` health | Startup capability probe; visible "capturing / paused / unsupported on this compositor" status |
| 6 | Capture only runs while the UI window is open / dies on XWayland window close (CopyQ, cliphist, GPaste) | Capture runs in a supervised always-on daemon decoupled from any UI window; a dedicated hidden persistent selection/message window keeps X11 capture alive | `vbuff-daemon` (owns listener + heartbeat) | Test: close all UI windows, copy, assert capture continues; watchdog re-registers OS hooks |
| 7 | Ignoring concealed/transient pasteboard hints and storing passwords/2FA (Maccy, CopyQ, Ditto, GPaste, Flycut, Clipy) | Check every platform hint before storing - `org.nspasteboard.ConcealedType`/`TransientType`/`AutoGeneratedType`, Windows `ExcludeClipboardContentFromMonitorProcessing`/`CanIncludeInClipboardHistory`, KDE `x-kde-passwordManagerHint` - and skip flagged clips entirely, never writing to disk | `vbuff-platform` (hint detection), policy in `vbuff-core` | Zero-leak metric: 100% of OS-flagged clips never reach the on-disk store, verified by test asserting nothing reaches `vbuff-store` |
| 8 | Capturing from every source, no default exclude list, so password managers leak (Ditto, Flycut) | Ship a sane default deny-list (1Password, KeePassXC, Bitwarden, etc.) plus first-class per-app include/exclude rules, applied before persistence | `vbuff-core` (policy) + `vbuff-platform` (source attribution) | Zero-leak metric: 100% of copies from default-excluded password managers never persist, verified by test |
| 9 | Echo loops: re-capturing vbuff's own restored/written clips -> duplicates or infinite image loop pinning a core (GPaste, Clipy, CopyQ) | Fingerprint (blake3 hash + owner) every self-write, keep a short suppression window; on a self-write match bump the existing entry's timestamp at most, never insert | `vbuff-core` (fingerprint/dedup) + `vbuff-platform` | Test: restore/self-write a clip, assert no new row and no loop; one copied image yields exactly one entry |
| 10 | Reading only `text/plain`, discarding richer flavors permanently (Clipman, GPaste, Diodon, cliphist, Maccy, CopyQ) | Enumerate and store every co-offered representation atomically as one multi-flavor record; prefer `image/png`/`public.tiff` over an `<img>` html tag; decide plain-vs-rich at paste time | `vbuff-core` (Clip multi-representation record) + `vbuff-platform` | Test: copy content offering html+rtf+png+plain, assert all flavors retrievable and image-over-html preference |
| 11 | Delayed-rendering / promised data never realized -> blank, un-pasteable entries (Win+V, CopyQ) | Actively request (realize) promised/delayed-rendered bytes while the source still owns the clipboard; if realization fails mark the entry incomplete rather than storing an empty row | `vbuff-platform` | Test: at least one renderable representation has actual bytes before commit; no phantom rows |
| 12 | History wiped or pins lost on app update (Ditto, Maccy) | Treat the on-disk store as a version-independent contract: forward-only transactional migrations gated by stored `schema_version` (`PRAGMA user_version`), automatic pre-migration backup, refuse to start rather than wipe on failed migration | `vbuff-store` | Data-durability metric: migration test matrix asserts zero data loss; integrity check + auto-restore from backup on failure |
| 13 | Pins evicted by trimming logic or silently reordered (Maccy, CopyQ) | Pins live in a separate eviction-exempt class with an explicit stable rank/order column distinct from recency; excluded from all eviction queries | `vbuff-store` + `vbuff-core` (retention rules) | Test: fill past cap, assert pins survive and keep user order across restart and migration |
| 14 | Unbounded SQLite growth; deleted rows never reclaimed -> multi-GB bloat (Ditto) | Hard item-count and total-byte ceilings with continuous eviction; `auto_vacuum=INCREMENTAL` plus periodic full `VACUUM`; current DB size surfaced in settings | `vbuff-store` | Test: sustained insert/evict loop keeps file size bounded; bytes-reclaimed reported after bulk delete |
| 15 | Raw uncompressed images (10-30 MB Retina TIFF) bloat the DB (Ditto, Maccy) | Transcode large images to compressed PNG, store oversized blobs externally by content hash with only a thumbnail in the row; content-addressed dedup with reference counting + GC | `vbuff-store` (external blob store) + `image` crate | Test: copy large TIFF, assert row holds thumbnail + external ref, identical images dedup; orphan-blob GC on startup |
| 16 | Slow search / full scans as history grows to 30k-50k items (CopyQ, Maccy) | FTS5 full-text index over a normalized text projection, kept in sync transactionally with inserts/deletes; virtualized, paged, keyset-loaded picker (never `SELECT *` the table) | `vbuff-store` (FTS5) + `vbuff-gui` (virtualized list) | Performance metric: picker open and search-as-you-type stay under ~16ms/frame with 50,000+ items |
| 17 | Auto-paste silently fails, pastes into the wrong window, or leaks held modifiers (CopyQ, Win+V, Klipper) | Restore focus and wait for the target to be confirmed frontmost before injecting a real paste; clear physical modifier state first; surface a clear message on detected failure instead of a silent no-op | `vbuff-platform` (focus-restore + injection) | Cross-platform behavioral test: paste lands in the previously focused app; modifier state verified cleared; failure surfaces a message |
| 18 | Routing the live DB through a cloud-sync folder -> `.conflict` files and corruption (Ditto) | Keep the DB in a per-machine, owner-only app-data location; sync at the record level through a real conflict-resolving protocol, never the raw SQLite file (non-goal); warn if the DB path is inside a known cloud folder | `vbuff-store` (path) + `vbuff-sync` (record-level) + `directories` | Startup check warns on cloud-folder DB path; sync conflict test asserts conflict-free record merge |
| 19 | Insecure, paywalled, LAN-only, or dead-backend sync (CrossPaste LAN-only, Paste subscription/Apple-only, Clipt/1Clipboard dead backend) | Zero-knowledge E2E encrypted sync over the internet, explicitly opt-in, with SAS/QR pairing and a self-host/local-only option; no single vendor backend that can read clips or be killed (non-goal) | `vbuff-sync` + `ring`/`rustls`/`x25519`+`chacha20poly1305` | Sync metric: clip appears on a paired device in a few seconds, E2E encrypted; relay sees only ciphertext, verified by test |
| 20 | Plaintext, world-readable history; no auth gate; recoverable "deleted" secrets in freelist/WAL/journal (Ditto, CopyQ, Win+V) | Create 0700/0600 data files in correct per-OS paths; encrypt sensitive items at rest; scrub deleted secrets with `secure_delete` + WAL-truncate + `VACUUM`; optional unlock gate for history | `vbuff-store` + `directories` + crypto crates | Permission audit on data dir/files; post-delete scan asserts no recoverable plaintext residue |
| 21 | Capture silently dying - listener crash, broken Windows viewer-chain, GNOME Wayland unsupported, second manager fighting for the chain (Win+V, ClipboardFusion, CopyQ, Ditto) | Supervised capture component with a heartbeat/watchdog that re-registers OS hooks (modern `AddClipboardFormatListener`, not the fragile `SetClipboardViewer` chain); detect second managers and tolerate them via fingerprinting; always show a "capturing / paused / unsupported" status | `vbuff-daemon` (watchdog) + `vbuff-platform` | Self-test confirms listener registered; heartbeat after a known self-write detects a stalled chain and re-subscribes; visible health indicator makes silent loss impossible |
| 22 | PRIMARY vs CLIPBOARD selection confusion floods or misses copies on Linux (X11 core, GPaste, Klipper, CopyQ) | Treat PRIMARY and CLIPBOARD as distinct sources; capture CLIPBOARD by default, make PRIMARY opt-in and debounced so mere selection does not flood history | `vbuff-platform` (Linux backend) | Test: select-to-copy does not create entries by default; opt-in PRIMARY only commits after the selection settles |
| 23 | Capture path blocks on huge payloads, pinning a core / OOM-crashing the monitor (CopyQ) | Stream clipboard bytes off the event thread with a configurable size cap; above the cap store a preview + external reference; guard reads and survive allocation failure without killing capture | `vbuff-platform` + `vbuff-store` | Test: copy hundreds of MB, assert capture thread stays responsive and the monitor does not crash |
| 24 | No source-app attribution, so trust rules and leak audits are impossible (Diodon, GPaste, Klipper) | Record owner/foreground app at capture (bundle id on macOS, owner PID -> executable on Windows, data-source client on Wayland where exposed); mark "unknown" explicitly where the compositor cannot attribute rather than guessing | `vbuff-platform` + `vbuff-store` (indexed metadata) | Per-clip source stored and filterable; Wayland-unknown case asserted as explicit, not silently "safe" |
| 25 | X11/Wayland/XWayland clipboard split not bridged, missing copies from the other world (Diodon, CopyQ) | Watch the Wayland selection (data-control) and the X11/XWayland selection (XFIXES) simultaneously, dedup across them by content fingerprint, and offer items into whichever world the paste target lives in | `vbuff-platform` (Linux backend) | Cross-world test: copy in a Wayland-native app, paste in an XWayland app and vice versa, with no duplicate entry |

Cross-cutting guarantees that back the table: pure behavior runs against the same fakes on every build host, while each promoted native adapter must supply independent real-host evidence. The active support claim is Windows only. `vbuff-core` holds dedup/fingerprint/retention/concealment policy as testable logic with no I/O, and the resident runtime exposes visible capture health so silent observed-state loss cannot hide behind shared-trait tests.

---

## Related documents

- [README.md](README.md) - project overview, build & usage
- [plan.md](plan.md) - phased implementation plan
- [recommendation.md](recommendation.md) - prioritized product & engineering recommendations
- [docs/implementation-batch-001-050.md](docs/implementation-batch-001-050.md) - execution status and review evidence for engineering backlog items 1-50
- [docs/implementation-batch-051-100.md](docs/implementation-batch-051-100.md) - execution status and review evidence for engineering backlog items 51-100
- [docs/implementation-batch-101-150.md](docs/implementation-batch-101-150.md) - release, Trust UI, migration/sync, product-policy, and review evidence for items 101-150
- [docs/implementation-batch-151-200.md](docs/implementation-batch-151-200.md) - privacy/AI, integrations, delivery gates, data freeze, and Compose evidence for items 151-200
- [docs/implementation-batch-201-250.md](docs/implementation-batch-201-250.md) - workflow contracts, popup design/accessibility, schema 6 lifecycle, and review evidence for items 201-250
- [docs/implementation-batch-251-300.md](docs/implementation-batch-251-300.md) - everyday runtime UX, sync/device and external integration foundations, operations, and review evidence for items 251-300
- [docs/implementation-batch-301-350.md](docs/implementation-batch-301-350.md) - trust/recall, schema 7 lifecycle, desktop fit, and review evidence for items 301-350
- [docs/decision-gates-151-200.md](docs/decision-gates-151-200.md) - numeric stop/go criteria, owner roles, external evidence, and dependency fallback ladders
- [docs/decision-gates-201-250.md](docs/decision-gates-201-250.md) - plugin host, native caret, assistive technology, display, and encrypted-recovery gates
- [docs/decision-gates-251-300.md](docs/decision-gates-251-300.md) - native auto-pause, live sync/client authority, release evidence, migration, and governance gates
- [docs/decision-gates-301-350.md](docs/decision-gates-301-350.md) - trust activation, recall persistence, lifecycle mutation, and native desktop gates
- [docs/limitations.md](docs/limitations.md) - versioned current limitations, workarounds, and exit evidence
- [docs/maintainer-handoff.md](docs/maintainer-handoff.md) - release custody, emergency patch, dependency, and sunset playbook
- [docs/scope-review.md](docs/scope-review.md) - quarterly dispositions and mechanical cut line
- [docs/data-contract-v1.md](docs/data-contract-v1.md) - frozen schema/hash/format/IPC fixtures and compatibility procedure
- [docs/data-contract-v2.md](docs/data-contract-v2.md) - schema 6 lifecycle and compatibility contract
- [docs/data-contract-v3.md](docs/data-contract-v3.md) - schema 7 lifecycle annotations, quarantine/export, and compatibility contract
- [docs/product-strategy-decisions.md](docs/product-strategy-decisions.md) - explicit resolution of conflicting licensing/pricing/governance hypotheses 128-140
- [docs/competitive-analysis.md](docs/competitive-analysis.md) - competitor landscape
- [docs/features-top-500.md](docs/features-top-500.md) - 640-feature catalog
- [docs/ideas-top-300.md](docs/ideas-top-300.md) - user-facing, sync, integration, and operations ideas 198-300
- [docs/ideas-301-400.md](docs/ideas-301-400.md) - extended privacy, search, storage, platform, team, automation, and governance ideas 301-400
- [docs/ideas-401-500.md](docs/ideas-401-500.md) - review backlog: current problems, SOLID/DRY cuts, design issues, quality gaps, and roadmap hygiene
- [docs/ideas-501-600.md](docs/ideas-501-600.md) - evidence-backed native correctness, Unicode/search, security, local-first sync, and verification ideas 501-600
- [docs/ideas-601-610.md](docs/ideas-601-610.md) - post-600 evidence-backed candidates outside the active execution goal
- [docs/ideas-611-620.md](docs/ideas-611-620.md) - second post-600 pass covering invariant-safe state, deterministic evidence, and key lifecycle
- [docs/repositories-research-100.md](docs/repositories-research-100.md) - 100 high-signal repositories and the primary research/standards evidence catalog
- [docs/mistakes-top-500.md](docs/mistakes-top-500.md) - 638 competitor anti-patterns and vbuff's fixes
- [docs/code-audit-top-50.md](docs/code-audit-top-50.md) - top 50 things wrong in the current code, cross-referenced against this document's claims
- [docs/problems-improvements-top-500.md](docs/problems-improvements-top-500.md) - items 51-556: the SOLID/DRY, security, platform, storage, concurrency, performance, testing, code-quality, config, GUI-design, docs, and dependency findings behind the "Current module map" and "Design system" sections above
- [docs/competitor-extras.md](docs/competitor-extras.md) - additional/advanced competitor features

---

## Backlog execution batches

The numbered backlog remains the canonical statement of intent; implementation status is kept separately so proposal text is never rewritten into a misleading completion claim.

| Range | Execution state | Canonical evidence |
|---|---|---|
| 1-50 | First implementation/review batch complete at the runtime, foundation, adapted, or explicit native-required level | [Batch 001-050 ledger](docs/implementation-batch-001-050.md) |
| 51-100 | Second implementation/review batch complete with native and runtime dependencies kept explicit | [Batch 051-100 ledger](docs/implementation-batch-051-100.md) |
| 101-150 | Third implementation/review batch complete with native, release-credential, transport, and policy dependencies explicit | [Batch 101-150 ledger](docs/implementation-batch-101-150.md) |
| 151-200 | Fourth implementation/review batch complete with runtime, foundation, adapted, native, and external dependencies explicit | [Batch 151-200 ledger](docs/implementation-batch-151-200.md) |
| 201-250 | Fifth implementation/review batch complete with runtime, foundation, adapted, native, and key-provider dependencies explicit | [Batch 201-250 ledger](docs/implementation-batch-201-250.md) |
| 251-300 | Sixth implementation/review batch complete with everyday runtime, device/integration foundations, and operational evidence dependencies explicit | [Batch 251-300 ledger](docs/implementation-batch-251-300.md) |
| 301-350 | Seventh implementation/review batch complete with runtime, foundation, adapted, native-required, and explicit release-gate dispositions | [Batch 301-350 ledger](docs/implementation-batch-301-350.md) |
| 351-600 | Queued in groups of 50 | Shared range map below |

The architectural cut line is strict: a pure algorithm can be complete as a foundation without being a shipped feature. In particular, native provenance/generation/realization work is not complete through `arboard`, and sync is not a user feature until authenticated transport, persistence, pairing UX, and replication are wired.

---

## 600-point review backlog map

The backlog is intentionally split instead of duplicated. `architecture.md`, `README.md`, `recommendation.md`, and `plan.md` should all point to the same ranges so the docs stay DRY while still making the full 600-item review easy to navigate. The final range is traceable to repository and primary-source evidence rather than popularity alone.

| Range | Canonical file | Ownership lens |
|---|---|---|
| 1-113 | [architecture.md](architecture.md) | Engineering architecture, native backends, data model, security, sync, testing |
| 114-197 | [recommendation.md](recommendation.md) | Product bets, positioning, monetization, roadmap tradeoffs, integrations |
| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) | Power-user workflows, UI/UX, everyday quality, sync, operations |
| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) | Privacy, search, storage, platform fit, teams, automation, governance |
| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) | Current problems, SOLID/DRY refactors, test gaps, designer-grade UX, review hygiene |
| 501-600 | [docs/ideas-501-600.md](docs/ideas-501-600.md) | Native protocol correctness, international text/search, security, local-first sync, evidence and verification |

The post-600 candidates [601-610](docs/ideas-601-610.md) and [611-620](docs/ideas-611-620.md) remain a separate evidence pool and do not change the canonical 1-600 objective.

---

## Ideas and improvements backlog (engineering & architecture)

> Items 1-113 of a 600-idea backlog. Companion lists: product/strategy ideas (114-197) in [recommendation.md](recommendation.md), user-facing/operations ideas (198-300) in [docs/ideas-top-300.md](docs/ideas-top-300.md), extended ideas (301-400) in [docs/ideas-301-400.md](docs/ideas-301-400.md), review backlog items (401-500) in [docs/ideas-401-500.md](docs/ideas-401-500.md), and evidence-backed ideas (501-600) in [docs/ideas-501-600.md](docs/ideas-501-600.md). Effort tags: `S`/`M`/`L`.

### Capture & monitoring engine

1. **Provenance-grade source attribution (window + URL + selection rect)** `[L]` - Extend SourceApp beyond bundle id/exe/title to capture, at the changeCount/WM_CLIPBOARDUPDATE instant, the focused browser tab URL (via AX/UIA tree walk on macOS/Windows), the document path for editors, and the on-screen selection rectangle, storing them as structured provenance on the CaptureEvent. _Value: Turns 'from Safari' into 'from github.com/foo/pull/3 in Safari', enabling far richer filtering, dedup-by-origin, and later 'reopen source' actions; this is the single most useful metadata users cannot get from any competitor and it is captured only at copy time when it is cheap._
2. **Capture lineage IDs to break cross-tool echo loops** `[M]` - Stamp every vbuff write with a per-write nonce embedded in a sentinel flavor AND record the content_hash of clips we wrote; the gate suppresses re-capture not just for the immediate self-write but for any echo that arrives within a bounded window carrying our nonce or matching a recently-written hash, covering KDE Connect / Universal Clipboard / another clipboard manager re-broadcasting our paste. _Value: The architecture's single-sentinel approach defeats only the immediate self-write; in a world with OS cloud-clipboard and a second manager running, a vbuff paste can loop back through a different transport with the sentinel stripped. Hash-and-nonce ledger closes that loop without dropping genuine re-copies of the same text by the user (those arrive outside the TTL window)._
3. **Adaptive macOS poll cadence driven by activity, not a fixed timer** `[M]` - Replace the fixed 150-250ms changeCount poll with a cadence that tightens to ~120ms for a few seconds after any keyboard Cmd/Ctrl activity or app-switch the daemon can observe, and relaxes toward 750ms during sustained idle, all while a single torn-read re-check guards correctness. _Value: Cuts wakeups (battery/energy) during the 95% of time nothing is copied, while keeping perceived latency low exactly when a copy is likely; a flat 200ms timer pays the full energy cost 24/7 for the rare copy. Directly serves the 'idle near 0% CPU' design goal better than the documented static back-off._
4. **Two-phase coalescing window keyed by flavor-set growth** `[M]` - Make burst coalescing close the debounce window early once the offered flavor set stops growing across two consecutive notifies (rather than waiting the full 40-80ms timer), and conversely extend it if a large/promised flavor is still being realized. _Value: Multi-format writers (Office, Electron) finish publishing flavors at very different speeds; a fixed timer either captures a half-published set (too short) or adds latency to every copy (too long). Growth-driven closure captures the complete set with minimum added latency, fixing the most common cause of 'pasted only plain text, lost the HTML' reports._
5. **Realization receipts: never commit a flavor we could not actually read** `[M]` - For promised/delayed-render and INCR flavors, attach a per-flavor realization status (Realized / Failed / Truncated) to each Flavor at capture; the gate commits the item only if at least one renderable flavor is fully Realized, and marks the rest as incomplete rather than storing empty/torn bytes. _Value: Phantom blank entries from un-realized promises are a named competitor mistake (Win+V, CopyQ). Per-flavor receipts make 'incomplete capture' a first-class, visible state instead of a silently corrupt row, and let the UI offer 'retry realization while source still open'._
6. **Redundant-flavor pruning at capture (semantic, not byte, dedup)** `[L]` - Before storing, drop flavors that are losslessly derivable from a richer sibling already present (e.g. a plain-text flavor byte-identical to the text extracted from the stored RTF/HTML, or a CF_DIB when an identical PNG is also offered), keeping a marker so paste-back can still synthesize them on demand. _Value: Office/browser copies routinely ship 5-8 overlapping flavors; storing all verbatim bloats the row and the encrypted CAS for zero added fidelity. Semantic pruning shrinks storage and dedup surface while the byte-for-byte guarantee is preserved for the canonical flavor. Distinct from the existing content_hash dedup, which only dedups whole identical clips._
7. **OTP/short-code capture with self-destruct TTL instead of skip** `[M]` - When the secret detector flags a one-time code (short numeric/auth-app pattern), still capture it but as an ephemeral clip with a hard 60-120s self-destruct timer and never-synced/never-evicted-to-disk-spill flag, rather than refusing capture outright. _Value: The catalog's approach is to skip OTPs entirely (#340), but users genuinely want to paste the code they just received into another field seconds later, then have it vanish. A short-TTL ephemeral capture serves the real workflow while still guaranteeing the secret evaporates - a privacy posture no competitor offers._
8. **Concealed-but-confirm: capture skipped sensitive clips into a volatile recovery slot** `[S]` - When the gate skips a clip for a concealed/secure-input/pattern reason, instead of total drop, keep only its existence and reason (no bytes) in a tiny in-memory ring of the last few skips, so the user can hit 'I meant to keep that last one' and re-grab it from the live clipboard if it is still owned. _Value: Fail-closed correctly drops false-positive skips too (a password-manager hint on something the user actually wanted, an over-aggressive regex). A byte-free, memory-only audit of recent skips lets users recover from an over-eager gate without ever weakening the fail-closed default or writing the secret to disk._
9. **Coalesce paste-and-recopy churn from format-converters** `[M]` - Detect the pattern where an app reads the clipboard and immediately rewrites a transformed version (e.g. a Markdown/HTML converter, a 'paste as plain text' tool) by tracking that the new clip's text is a normalized transform of the immediately-prior one, and offer to fold it into the prior entry as an alternate representation rather than a separate history row. _Value: Power users running clipboard-transform utilities generate cascades of near-duplicate rows; growing-selection merge (#563) only catches prefix extensions, not transform pairs. Folding transforms keeps history clean and preserves both forms under one logical copy._
10. **Capture-engine self-test on startup and on backend restart** `[M]` - On daemon start and after any watcher auto-restart, run a silent round-trip probe: write a unique sentinel value to the clipboard, confirm the backend observes exactly one coalesced event attributed to self and suppresses it, then restore the prior contents, surfacing a health badge if the listener is not actually delivering events. _Value: Silent capture death (OS dropped the format listener, Wayland portal revoked, X11 selection lost) is invisible until the user notices nothing is being saved - a top trust-killer. An active self-test makes a dead capture path detectable immediately instead of after hours of lost copies, and validates the echo-suppression path stays wired after a restart._
11. **Per-source capture policy resolved at the gate, with title/URL predicates** `[M]` - Generalize per-app rules into ordered predicates that match on the rich provenance (app + window title regex + URL host) and resolve to a capture action (skip / capture-plain-only / capture-sensitive / strip-images) inside the existing CaptureDecision gate, evaluated before content scanning. _Value: Per-app rules (#31) are too coarse: users want 'never store images from Slack', 'force plain text from the terminal', 'treat anything from *.bank.com as sensitive'. Title/URL-scoped predicates make policy precise without new code paths, reusing the audited single-gate design and the new provenance metadata._
12. **Monotonic capture journal with seq-gap detection** `[S]` - Persist the backend's monotonic change seq alongside each committed clip and, on the consume loop, detect and log gaps (a changeCount jumped by N but we only captured M) so missed/contended captures become an observable metric rather than silent loss. _Value: changeCount/sequence numbers advance on every system write including ones we lost to OpenClipboard contention or a too-slow read; without gap tracking these losses are invisible. A seq-gap counter turns 'why didn't my copy get saved' from an unfalsifiable complaint into a diagnosable, fixable event and feeds the health badge._
13. **PRIMARY-selection capture with intent gating instead of hard debounce** `[M]` - On Linux, rather than only debouncing the noisy PRIMARY selection, gate PRIMARY captures behind a brief settle plus an intent signal (selection held stable past a threshold AND a modifier or middle-click observed, where detectable), promoting a PRIMARY selection to history only when it looks deliberate. _Value: PRIMARY changes on every drag, so the catalog's 'debounce hard' (#520) still floods history with accidental partial selections. Intent gating captures the selections users actually meant to keep, making PRIMARY capture usable rather than a noise generator users disable._
14. **Flavor-integrity hashing to detect torn multi-flavor reads** `[M]` - Hash each flavor independently at read time and re-verify the changeCount/owner did not change between the first and last flavor read; if it did, recompute only the changed flavor or discard the generation, guaranteeing the stored flavor set came from one coherent clipboard generation. _Value: The macOS change-count race is documented for the aggregate read, but a multi-item, multi-flavor pasteboard can be partially overwritten mid-enumeration, yielding a Frankenstein clip (HTML from copy A, image from copy B) that the single end-of-read re-check can miss. Per-flavor coherence checking eliminates torn mixes that corrupt paste fidelity._

### Storage, data model & search

15. **Per-kind FTS5 tokenizer with a code-aware secondary index** `[M]` - Run two FTS5 columns over item_text - the existing unicode61 column for prose plus a separate trigram-tokenized column for code/identifier clips so 'getUser' matches 'getUserById', 'snake_case' fragments match, and symbol-heavy text stops being unsearchable. _Value: FTS5 unicode61 splits on punctuation, so today copying a function name or a UUID makes it findable only as a whole token; developers are the core audience and currently get the worst search. Trigram is built into modern SQLite, so it is a schema add, not a dependency._
16. **Scheduled FTS5 'optimize' + tuned automerge to stop index bloat** `[S]` - Treat the FTS5 index as a maintained structure - set 'pragma item_fts(automerge=4)' and run 'INSERT INTO item_fts(item_fts) VALUES(optimize)' during the same idle window as incremental vacuum, gated by a dirty-segment counter so it only runs after meaningful churn. _Value: A clipboard manager does thousands of insert/delete cycles a day (capture + eviction); unmaintained FTS5 accumulates b-tree segments that slow queries and waste pages inside the encrypted DB. Nobody has scheduled FTS maintenance in the catalog, only generic VACUUM._
17. **SimHash near-duplicate fingerprint column for fuzzy dedup** `[M]` - Store a 64-bit SimHash (over shingled normalized text) alongside the exact BLAKE3 hash, indexed in 4 banded prefix columns, so capture can detect 'almost the same clip' (re-copied with a trailing space, a changed line, reflowed whitespace) within a small Hamming distance. _Value: Exact BLAKE3 dedup misses the most common real duplication: the same paragraph re-copied after a tiny edit, which floods history. This powers the catalog's vague 'merge growing selection' / 'smart duplicate-merge' items with an actual algorithm and a cheap indexed lookup instead of a full scan._
18. **Image perceptual hash (dHash) for visual near-dup collapse** `[M]` - Compute a 64-bit difference-hash from the thumbnail of every image clip and store it indexed, so re-screenshotting the same window, or pasting a recompressed copy of an image, collapses onto the existing item even though the PNG bytes (and BLAKE3) differ. _Value: Screenshots are a top image source and byte-level dedup never catches them because re-encoding changes every byte; users drown in near-identical screenshots. The thumbnail cache already exists, so the hash is nearly free to compute at capture._
19. **Local quantized embedding index for opt-in semantic search** `[L]` - Make the catalog's one-line 'semantic search [future]' concrete: a small local model (e.g. a 384-dim MiniLM via fastembed/ONNX) writes int8-quantized vectors into a sqlite-vec virtual table, populated lazily on idle, queried as a re-rank stage over the FTS-narrowed candidate set. _Value: Turns a hand-wave into a buildable spec with named dimensions, quantization, and a hybrid (FTS-then-vector) query plan that fits the existing 'FTS narrows, Rust ranks' pipeline. Int8 + 384-dim keeps the index ~400 bytes/clip, viable even at 50k items in an encrypted DB._
20. **Generated structured-data columns for typed clips** `[M]` - On capture, classify and extract typed fields (URL host, hex color, IBAN/card BIN masked, ISO date, code language) into generated/extra columns and a small kv side-table, so 'host:github.com', 'color:#ff0', or 'lang:rust' filters resolve by index instead of regex scan. _Value: Makes the catalog's prefix operators (type:, app:, before:) extensible to content-derived facets that LIKE/regex can't index, and keeps the canonical bytes untouched. Indexed facet filtering stays fast where whole-table regex degrades._
21. **CAS refcount table instead of refcount-by-query GC** `[M]` - Replace the architecture's 'refcount-by-query plus 60s grace window' blob GC with an explicit blob_ref(hash, refcount) table maintained transactionally by insert/delete triggers, so blob liveness is O(1) and a crash can't strand or prematurely reap a blob. _Value: Refcount-by-query scans flavor on every GC and the grace window is a race-prone heuristic; an explicit transactional refcount is both faster and provably correct, and it makes 'how much does this blob save via dedup' queryable. CAS GC correctness is currently the weakest part of the spill design._
22. **Tiered/sharded CAS layout with content-type prefix** `[S]` - Extend the flat blobs/<2hex>/<hash> layout to blobs/<kind>/<2hex>/<hash> plus a generation-fanout so directories stay under a few thousand entries, and store the spill threshold per-kind (text spills later, images/video sooner) rather than one global 256 KiB. _Value: A single global threshold over-inlines large compressible text (bloating DB pages) while a flat hash dir can hit tens of thousands of files in one folder on heavy users, hurting some filesystems. Per-kind thresholds keep row scans and FTS fast where it matters._
23. **Migration dry-run + checksum manifest with auto-rollback** `[M]` - Before each forward migration, record a manifest (pre-version, row counts, schema hash) and run the migration against a temp copy first; on success swap atomically, on any error restore the temporary safety copy and surface a precise 'migration N failed at step K' instead of a half-applied DB. _Value: The current migration guard can recover a failed apply from a temporary owner-only artifact and removes that artifact after successful verification; it is not a durable rollback backup. A dry-run + manifest would make upgrade failure more diagnosable without changing that retention boundary._
24. **Compressed mirror table for the FTS text projection** `[M]` - Store item_text as an FTS5 external-content table whose backing column is zstd-compressed with a trained dictionary over clip text, decompressing only for snippet()/highlight() of the visible page. _Value: The searchable surrogate text (e.g. 'PNG 1920x1080 from Safari') plus full text of every clip is duplicated for FTS; at 50k clips this projection dominates DB size. A shared zstd dictionary over short, similar clip strings yields large savings the page-level SQLCipher cipher can't get._
25. **Keyset-cursor search state cached per popup session** `[S]` - Cache the last keyset (updated_at, seq) cursor and the compiled FTS query per open popup so paging down a long filtered result never re-runs the match from row zero, and so reopening the popup restores scroll position without an OFFSET scan. _Value: Keyset pagination is already chosen, but each new page currently re-issues the full bm25-ranked query; caching the cursor + query plan keeps deep scrolls O(page) and makes type-to-filter feel instant at 50k rows. Directly improves the headline 'sub-frame search' promise._
26. **Adaptive search-tier switch driven by row count and latency** `[M]` - Auto-promote from the substring (LIKE/instr) tier to the FTS5 tier per-query based on live history size and a measured p95 latency budget, with a one-time backfill of item_fts when the threshold is first crossed, instead of a static config flag. _Value: The design ships substring as the tier and 'switches FTS on in v1' manually; an adaptive switch means small histories pay zero FTS write cost while large ones never hit the LIKE cliff, with no user knob. Removes a guess about where the crossover is._
27. **Bloom-filter pre-check to skip the dedup SELECT on cold inserts** `[S]` - Keep an in-memory Bloom filter of recent content hashes (rebuilt from the hash index at startup) so the common 'brand new clip' path skips the SQLite dedup SELECT entirely, only querying the DB on a Bloom hit to confirm. _Value: Every capture currently does a SELECT-by-hash before insert even though most copies are novel; on the single-writer hot path that round-trip adds latency and page touches. A Bloom filter makes the negative case allocation-free and keeps capture off the disk for new content._
28. **Content-hash chain audit + self-heal for silent dedup corruption** `[M]` - Add a periodic background check that recomputes content_hash for a rolling sample of rows and verifies it matches the stored hash and the partial unique index, quarantining and re-hashing any mismatch (e.g. from a botched migration or bit-rot) rather than silently serving wrong dedup. _Value: Dedup correctness is a stated test crown-jewel invariant, but nothing verifies it on real long-lived DBs where a migration bug could desync hash and bytes; a rolling audit catches the one failure (silent wrong dedup / lost-to-dedup clip) that users would never notice until data is gone._

### Sync, P2P & cryptography

29. **Per-item OR-Set tombstones instead of raw LWW delete loss** `[L]` - Replace the plan's pure Lamport+LWW with an add-wins observed-remove set for the item lifecycle (pin/unpin, tag add/remove, delete), so a delete on device A and a re-pin on device B converge to a single defensible state instead of one silently clobbering the other. _Value: Pure LWW on a clipboard is data loss in disguise: deleting an item on your phone while pinning it on your laptop should not race to a coin-flip. An OR-Set keeps merges intuitive (a concurrent delete+pin keeps the pinned item) and kills the 'my clip vanished after sync' class of bugs that plagues Ditto-style tools._
30. **Field-level last-writer-wins register per mutable column** `[M]` - Give each independently editable field (custom name, notes, color, pinned flag, tags) its own Lamport register inside sync_log rather than one clock per item, so editing the note on A and the color on B both survive a merge. _Value: Item-granular LWW throws away the loser's entire edit even when the two devices touched different fields. Column-granular registers make concurrent edits compose, which is exactly what users expect from 'one clipboard everywhere' and what a single item-level clock cannot deliver._
31. **Hybrid logical clocks (HLC) to defend against device clock skew** `[M]` - Carry a Hybrid Logical Clock (wall-time ceiling + Lamport counter) on every sync record so LWW tie-breaks track real human ordering and a device with a wildly wrong system clock cannot permanently win or lose every conflict. _Value: Plain Lamport tie-breaks are arbitrary and a single misconfigured clock (or a daylight-saving jump) makes one device's edits always shadow others. HLC keeps causality correct while bounding wall-clock divergence, with no extra round trips, so cross-device merges feel chronologically sane._
32. **Pairing-time key-transparency log with SAS over the whole device set** `[L]` - Maintain an append-only hash chain of (device_uuid, pubkey, added_by, lamport) entries, and at pairing have the SAS short-string commit to the current chain head, not just the two pubkeys being exchanged. _Value: Two-party SAS proves the link you are forming but says nothing about the third device an attacker may have already injected into your trust set. Binding SAS to the full membership chain head turns every pairing into an implicit audit of the entire fleet, catching silent device injection._
33. **Device revocation with epoch-based group rekey** `[L]` - Model the synced set as a key-agreement group with an epoch counter; revoking a lost device bumps the epoch, distributes a fresh group key wrapped to remaining devices, and refuses to decrypt new records under the dead device's epoch. _Value: The plan stores per-device pubkeys but has no answer for 'my laptop was stolen.' Without group rekey a revoked device that still holds the transport keys keeps reading everything synced after the theft. Epoch rekey gives a real, testable 'cut off device X now' button, the table-stakes feature every E2E system needs._
34. **Post-compromise recovery via scheduled DEK rotation and re-wrap** `[L]` - Rotate the root data-encryption key on a schedule or on demand, re-wrapping (not re-encrypting) historical records' content keys so a one-time key leak stops granting access to clips copied after the rotation. _Value: A static root DEK for the lifetime of the install means a single keychain exfiltration is game over forever. Cheap re-wrap rotation (each item already has a per-blob subkey) bounds the blast radius of a compromise to a window, which is the difference between a scare and a catastrophe for a tool that holds every password you ever copied._
35. **Sealed-sender relay envelopes that hide the sender device** `[M]` - For the v2 relay, wrap each ciphertext in a sealed-sender envelope so the relay learns neither plaintext nor which device sent it, only an opaque per-group routing tag rotated each epoch. _Value: A zero-knowledge relay that still sees 'device A talked to device B at 14:03, 200 bytes' leaks a behavioral graph of when and from where you copy. Sealed sender plus rotating routing tags makes the relay's metadata genuinely uninteresting, strengthening the 'never a backend that can profile you' promise beyond just payload secrecy._
36. **Selective-sync policy DSL evaluated before encryption** `[M]` - A small declarative rule set ('text under 4KB syncs everywhere; images only to laptops; never sync items from app=1Password') compiled to a predicate that runs at capture time, before a record is ever sealed for transport. _Value: The plan offers coarse per-type and per-source toggles; power users want 'images to my desktop but not my phone's limited storage' and 'work clips stay on work devices.' Deciding eligibility before encryption guarantees ineligible clips are never even enveloped, so the policy is enforced by construction, not by a downstream filter that could be bypassed._
37. **Per-device sync lanes (asymmetric subscriptions)** `[M]` - Let each device subscribe to a subset of content classes and collections rather than the whole history, with the sender consulting the recipient's advertised lane before pushing, so a phone can hold only pinned text while a desktop holds everything. _Value: Forcing every device to carry the full history is wrong for asymmetric fleets (a phone should not mirror 50k desktop clips). Lanes make sync scale to mixed hardware and storage budgets, and they pair naturally with the selective-sync policy to give a coherent 'who gets what' model instead of all-or-nothing replication._
38. **Anti-entropy reconciliation via Merkle range digests** `[L]` - Instead of only push-on-copy, periodically reconcile two devices by exchanging a Merkle tree of sync_log ranges (bucketed by Lamport/HLC) and pulling only the differing leaves, so a device that was offline for a week catches up in one logarithmic round. _Value: Push-on-copy silently diverges the moment a device is asleep or off-LAN; the plan has no convergence story for the rejoin case. Merkle range reconciliation is the proven gossip-DB technique for 'find what we disagree on cheaply' and makes sync self-healing rather than fragile, directly addressing the 'sync just stopped' competitor failure._
39. **Verifiable wipe-propagation receipts** `[M]` - When a user issues 'wipe this clip everywhere,' each peer that applies the tombstone returns a signed receipt (device_uuid + item hash + epoch) the originator collects and surfaces, so 'deleted on all 3 devices' is provable, not hopeful. _Value: Deleting a leaked secret is the single most safety-critical sync operation, yet fire-and-forget tombstones give no confirmation it actually landed on the phone that was offline. Signed receipts turn remote wipe into an auditable action, which matters enormously for a tool whose deletes are often 'I copied a password I should not have.'_
40. **Out-of-band recovery key (BIP39-style) decoupled from the keychain** `[M]` - Generate a 24-word recovery phrase at setup that can re-derive the group-membership root, so a user whose OS keychain is wiped (reinstall, dead disk) can rejoin their own synced set instead of being locked out forever. _Value: Tying everything to the OS secret store means a clean reinstall orphans the user from their own E2E history with no escape hatch, the classic E2E onboarding cliff. A printable, offline recovery phrase makes account-less zero-knowledge sync actually recoverable, which is the difference between 'private' and 'lose everything once.'_
41. **QR-based offline bootstrap that ships the first encrypted snapshot** `[M]` - Extend QR pairing so the QR (or a follow-up animated QR / local handoff) carries not just the key exchange but a compressed encrypted seed of pinned items and snippets, so a freshly paired device is useful instantly without waiting on relay/LAN catch-up. _Value: Cold-start after pairing is a dead-air moment: the new device is trusted but empty until anti-entropy runs. Bootstrapping the high-value subset (pins, snippets) inline with pairing makes 'add my new laptop' feel instant, turning the pairing flow from a chore into the product's wow moment._
42. **Tamper-evident local sync ledger ('what synced, to whom, when')** `[M]` - Keep an append-only, hash-chained local ledger of every sync event (item hash, peer, direction, epoch, decision) the user can review, so 'did my bank OTP ever leave this machine?' has a concrete, verifiable answer. _Value: Private-by-default is only believable if it is inspectable; right now a user must trust that exclusion rules worked. A signed local audit trail lets a privacy-conscious user (the exact buyer this product targets) verify the sync boundary held, and it doubles as the forensic record if exclusion ever has a bug._
43. **Capability-token pairing for one-way 'send to my other device'** `[M]` - Issue scoped, expiring capability tokens (e.g. a token that only grants push-to-device-B of a single item) so a borrowed or semi-trusted machine can send you a clip without being admitted as a full bidirectional sync peer. _Value: Full pairing is too heavy for 'paste this from the conference-room PC to my phone' and admitting that machine to your whole history is unsafe. Scoped one-shot tokens give a safe, revocable middle ground between fully-trusted device and no relationship, unlocking AirDrop-like handoff without widening the trust boundary._
44. **Padding and length-bucketing of sync payloads against size fingerprinting** `[S]` - Pad each transport envelope to the next size bucket (and optionally inject cover traffic) so an on-path observer of LAN or relay traffic cannot infer clip type or content from ciphertext length. _Value: AEAD hides bytes but not size; a 6-digit ciphertext is obviously an OTP and a multi-MB one is obviously an image, so length alone leaks the very classification the app works to protect. Bucketed padding closes a real side channel for a workload where item size is highly informative, hardening the E2E claim against traffic analysis._

### Performance, reliability & observability

45. **Capture-loss accounting ledger (the anti-silent-loss metric)** `[M]` - Maintain counters for every capture outcome: observed and stored, policy skip, owner contention, INCR truncation, oversize rejection, backpressure, self-write suppression, and detected native sequence gap. Surface only what can be known, such as 'N observed, M intentionally skipped, G sequence gaps'; never convert an unknowable overwritten payload into '0 lost'. _Value: this turns capture health into auditable evidence without claiming more observability than the OS provides._
46. **changeCount poll-skew adaptive scheduler (macOS)** `[M]` - Instead of a fixed 150-250ms poll, drive the macOS poll interval off observed copy cadence and recent miss-risk: tighten toward ~120ms during active typing/copy bursts, widen toward 750ms-1s when the changeCount has been stable and the machine is idle or on battery, with a hard wake on app-activation notifications. _Value: The catalogued 'battery throttle' (574) is binary; a cadence-aware scheduler cuts idle wakeups (the Maccy ~45% idle CPU complaint cited in recommendation.md) far harder while actually lowering miss-rate during the bursts that matter, which a flat interval cannot do._
47. **Per-subsystem CPU/wakeup budget with a tripwire** `[M]` - Assign explicit budgets (e.g. watcher <0.2% idle CPU, <X wakeups/min; store maintenance <Y ms/tick) and have a lightweight in-process sampler trip a logged warning + auto-backoff when a subsystem exceeds its budget over a rolling window. _Value: Idle CPU regressions are invisible until a user complains; an internal budget that self-reports converts 'near 0% as a multi-day metric' (an aspiration in the docs) into an enforced, alerting invariant that catches a leak or busy-spin the day it lands, not in a bug report._
48. **Structured tracing with a redacting span layer** `[M]` - Adopt a tracing/log schema where every span carries clip_id, byte_size, kind, and source_app but is wired through a redaction filter that guarantees clip *content* and secret-flagged metadata can never enter a log line, even at trace level. _Value: A clipboard manager's logs are uniquely dangerous (they sit next to every password); generic logging guidance ignores this, so a privacy-by-construction logging layer is both a reliability tool and a hard requirement that prevents debug logging from becoming the leak the whole encryption design is meant to prevent._
49. **In-memory metrics ring buffer dumped on crash** `[M]` - Keep the last N seconds of counters/latency histograms (capture rate, write-queue depth, FTS query times, retry counts) in a fixed-size lock-free ring in RAM, and flush it alongside a panic/abort to a crash report file so the moments *before* a crash are recoverable without persistent always-on disk logging. _Value: Crash recovery in the docs only covers DB durability, not diagnosing *why* the daemon died; a pre-crash metrics snapshot makes intermittent watcher deaths debuggable while keeping zero steady-state logging overhead and no sensitive content on disk._
50. **External heartbeat file for third-party supervision** `[S]` - Have the daemon atomically rewrite a small heartbeat file (timestamp, pid, capture-state, schema_version, last-capture-age) every few seconds so launchd/systemd/Task Scheduler watchdogs, monitoring scripts, or a `vbuff doctor` command can detect a wedged-but-not-crashed daemon that the internal watchdog can't see. _Value: The catalogued auto-restart watcher (580) only covers failures the process itself notices; a hung event loop or deadlocked SQLite writer keeps the process alive while capture is dead - exactly the GPaste/CopyQ 'pinned a core / stalled' class - and only an external liveness signal catches it._
51. **Two-stage watchdog: re-subscribe before restart** `[M]` - Make supervision tiered - on a detected stall, first attempt to re-register the OS clipboard hook and re-grab the hotkey in place (probing with a known self-write and confirming the echo), and only escalate to a full thread/daemon restart if the cheap re-subscribe fails, with each escalation logged. _Value: A blunt restart drops in-flight captures and resets focus state; the broken-Windows-viewer-chain and dropped-listener failures (mistake #21) are almost always fixable by re-subscribing, so a graduated response restores capture faster and with less collateral than the catalogued blanket auto-restart._
52. **Write-queue depth backpressure with shed-to-preview** `[L]` - When the bounded write queue crosses a high-water mark under a copy flood, degrade gracefully by enqueuing a lightweight preview+hash placeholder and materializing full flavors lazily, rather than the catalogued 'just batch and queue' which still buffers full payloads in RAM. _Value: Backpressure (582) as catalogued bounds the queue length but not the *bytes* in flight; a 700MB-copy storm (the CopyQ #3096 OOM) can blow memory even with a short queue, so shedding to previews under pressure is what actually keeps the watcher alive during the worst case._
53. **FTS index health monitor with incremental optimize** `[M]` - Track FTS5 segment count and query-latency drift over time and schedule `INSERT INTO item_fts(item_fts) VALUES('merge', N)` / bounded 'optimize' runs during idle, instead of letting the external-content index fragment unboundedly as tens of thousands of inserts/deletes accumulate. _Value: FTS5 over a churning external-content table degrades silently as segments pile up - the exact 'search slows as history grows to 30-50k' failure (mistake #16) the index was supposed to prevent; nothing in the catalog maintains the index after it's built, so this closes the gap between 'has FTS' and 'FTS stays fast.'_
54. **Tiered/aged history with a cold partition** `[L]` - Split storage so the hot recent window (what type-to-filter scans by default) stays small and fully indexed, while older non-pinned items roll into a compressed, less-indexed cold tier that's searched only on explicit 'search all history' - keeping steady-state working-set and FTS size bounded even in unlimited mode. _Value: The catalog offers only flat caps (count/size/age) or unlimited; power users who want long retention currently pay full query and memory cost on every keystroke, so a hot/cold split is the only way to offer 'keep everything' without the large-history scale tax that flat unlimited mode imposes._
55. **Startup self-test and `vbuff doctor` health surface** `[M]` - On launch and on demand, run a fast self-test (keychain reachable, DB integrity_check quick mode, FTS row-count vs item-count parity, orphaned-blob scan, OS hook registered, hotkey grabbed, encryption canary read-back) and expose results as both a tray health badge and machine-readable `vbuff doctor --json`. _Value: The catalogued diagnostics command (584, tagged 'future') is repair-focused and DB-only; a structured, scriptable health surface that also covers the platform hooks and encryption-engaged check is what lets a user (or a packaging QA matrix) confirm capture is actually live and at-rest crypto actually engaged - the canary-grep concern made continuous._
56. **Memory ceiling with explicit pressure response, not just LRU** `[M]` - Set a hard RSS target and react to OS memory-pressure signals (macOS dispatch memorypressure source, Windows memory-resource notifications, Linux PSI/cgroup) by proactively dropping decoded-image and thumbnail caches and trimming mmap, rather than only evicting reactively under a fixed LRU budget. _Value: The catalogued memory cap (575) is a self-imposed LRU number that ignores actual system pressure; honoring OS pressure signals makes vbuff a good citizen on a loaded machine and prevents the resident clipboard manager from being the process the OS kills first - addressing the Maccy memory-leak reputation directly._
57. **Deterministic crash-forensics for capture races** `[M]` - When the macOS changeCount race or an X11 INCR/torn-read is detected, record a structured anomaly event (sequence numbers, observed vs expected changeCount, byte counts per flavor) into a bounded forensic log so these inherently nondeterministic bugs can be diagnosed from field reports instead of being unreproducible. _Value: The architecture explicitly flags the changeCount race and INCR truncation as known traps but provides no way to know they fired in production; capturing the discriminating state turns 'occasionally a clip is torn' from an unfixable mystery into a reportable, fixable event without ever logging clip content._
58. **Benchmark-gated performance budgets in CI** `[M]` - Wire criterion benchmarks for the load-bearing hot paths (type-to-filter at 100k items, single-capture insert+FTS+evict latency, cold-start-to-hotkey-live, SQLCipher page-decrypt overhead) into CI with regression thresholds that fail the build on a budget breach. _Value: The docs assert sub-frame search at 100k and few-hundred-ms cold start as goals but nothing prevents a later commit from quietly blowing them; a benchmark gate turns these headline performance promises into invariants the merge process enforces rather than hopes hold._
59. **SQLCipher vs cold-start cost telemetry and KDF tuning record** `[M]` - Measure and record the actual page-decrypt and connection-open cost SQLCipher adds on first read, store the chosen KDF/cipher parameters in a versioned config record, and use the measured cost to tune mmap and the lazy-open path so encryption never silently regresses the cold-start budget. _Value: Full-DB encryption is committed and on by default, but its first-read decrypt cost directly fights the cold-start-responsiveness goal; measuring it (and pinning params explicitly) is the only way to keep both promises honest and to detect if an upstream SQLCipher change shifts the trade-off._

### Security & privacy hardening

60. **Process self-sandboxing after startup (seccomp / Landlock / App Sandbox / pledge)** `[L]` - Once the watcher, store, hotkey, and tray are initialized, drop the process into a per-OS sandbox that whitelists only the data dir, the keychain socket, and the LAN sync socket: seccomp-bpf + Landlock on Linux, the macOS App Sandbox entitlement set, and a restricted token / Job Object on Windows. _Value: A clipboard manager that has touched every password is a prime target; post-init confinement means an exploited parser (image/CF_HTML/INCR) cannot read ~/.ssh, exfiltrate over arbitrary sockets, or write outside the data dir. The threat model currently stops at file modes and zeroize; this closes the 'our own process is compromised' branch the architecture explicitly leaves open._
61. **Disable core dumps, crash reports, and ptrace on our own process** `[S]` - At startup set RLIMIT_CORE=0 + PR_SET_DUMPABLE(0) (Linux), suppress the macOS crash reporter / set PT_DENY_ATTACH equivalents, and disable Windows WER/minidumps, so a crash never spills decrypted clips or the in-memory DEK into a world-readable dump or a vendor crash-upload pipe. _Value: The docs note zeroize 'doesn't defend against swap' but say nothing about crash dumps, which are a far more common leak: a segfault in the egui/image path would otherwise serialize plaintext history and the live key to disk and potentially to Apple/Microsoft. Directly fills a hole in the 'memory scraping' threat-model row._
62. **Encrypted, locked-down swap/hibernation guidance plus mlock budget management** `[M]` - Extend the existing best-effort mlock to a real strategy: lock the DEK, the SQLCipher page cache headroom, and decrypted-clip buffers within RLIMIT_MEMLOCK, raise the limit where privileged, and surface a one-line 'your swap is unencrypted -> clips may hit disk' warning detected per-OS (encrypted swap on macOS, swap/hibernate file state on Windows/Linux). _Value: Swap is the named blind spot in the crypto section. Telling users honestly when their OS can page plaintext to disk, and locking what we can, turns an undefended path into an informed, mitigated one without overclaiming._
63. **Hardware-bound key wrapping (Secure Enclave / TPM 2.0 / TPM-backed DPAPI)** `[L]` - Add a SecretStore tier that wraps the future root DEK with a non-exportable hardware key: a Secure Enclave key on macOS, a TPM 2.0 sealed key on Windows/Linux, so the DEK can only be unwrapped on this physical machine even if the keychain blob is copied. _Value: Once SQLCipher and OS-keystore delivery exist, a software-readable keychain entry would still be accessible to a same-user process or stolen keychain export. Hardware binding would protect an encrypted database off-device; no current DEK or encrypted live database is implied._
64. **Biometric / passkey unlock gate (Touch ID, Windows Hello, FIDO2)** `[M]` - Let a future auto-lock flow be released by platform biometrics or a hardware passkey (LocalAuthentication on macOS, Windows Hello, libfido2 on Linux) as an alternative to a planned master password / PIN, releasing the in-memory DEK only after a successful local user-presence check. _Value: Password/PIN unlock is itself target work and would remain phishable/shoulder-surfable. A user-presence biometric gate would be faster and pairs naturally with hardware-bound key wrapping._
65. **Duress PIN / decoy vault for plausible deniability** `[L]` - Support a second 'duress' unlock secret that opens a decoy history (empty or seeded with innocuous clips) while leaving the real encrypted store inaccessible and indistinguishable on disk, optionally triggering a silent secure-wipe of the real DEK. _Value: For users under coercion (border crossing, theft-with-compulsion) the master-password model has no answer: you either unlock everything or refuse visibly. A decoy mode is a recognized high-assurance feature (VeraCrypt hidden volumes) and reinforces the 'private by default' positioning beyond what every rival offers._
66. **Entropy-based + structural secret detection with a tunable confidence gate** `[M]` - Augment the fixed regex detectors (cards/JWT/PEM/AWS) with a Shannon-entropy + charset-class scorer for generic high-randomness tokens, plus length/format heuristics, gated by a user-adjustable sensitivity threshold and a recall/precision corpus already in the test plan. _Value: The cataloged detectors only catch four known shapes; a 40-char random API token for any other service sails through as Normal. Entropy scoring generalizes the 'looks secret' decision to unknown credential formats, which is exactly the class of leak (a writer in an app we can't identify) the threat model admits it misses._
67. **Retroactive secret clawback: re-scan and reclassify already-stored clips** `[M]` - When the secret-detector ruleset is updated (new built-in pattern, new user regex, or a tightened entropy threshold), run a background pass over existing history to re-tag matches as Sensitive, apply masking + shorter retention, and secure-delete sync copies that already left the device. _Value: Detectors evolve, but a secret captured under yesterday's looser rules stays in plaintext-equivalent retention and may already be synced. Clawback makes a detector improvement actually protect historical data instead of only future captures, and is something no competitor does._
68. **Cooperate with OS-native clipboard history when staging sensitive writes** `[M]` - On future paste-back and internal copy paths, set the exact native history/monitor/cloud markers supported by the OS, including separate Windows local-history and cloud-upload policy, then verify the atomic write outcome before enabling sensitive delivery. _Value: the current generic backend can neither prove nor set these exclusions. A native writer with verified exclusion is required before that path can be enabled, independently of future SQLCipher protection inside vbuff._
69. **Tamper-evident local security audit log (HMAC-chained)** `[M]` - Maintain an append-only, HMAC-chained log of security-relevant events (unlock/lock, failed unlock attempts, key access, secure-wipe, sync pairing, detector-ruleset changes), keyed by an HKDF subkey so any deletion or reordering breaks the chain and is detectable on next open. _Value: There is currently no way for a user to notice that someone unlocked the vault at 3am or that pairing happened without them. A verifiable local log gives forensic visibility after a suspected compromise and underpins the 'prove it' marketing stance the recommendation leans on, without phoning home._
70. **Supply-chain CI gate: cargo-deny + cargo-vet + cargo-audit as a release blocker** `[S]` - Add a CI lane that runs cargo-audit (RustSec/OSV), cargo-deny (license + duplicate + advisory bans), and cargo-vet (reviewed-dependency attestations) on the pinned lockfile, failing the build on any unreviewed or vulnerable dependency before artifacts are produced. _Value: For a tool whose entire pitch is trust and longevity, a single compromised transitive crate (this app pulls in crypto, SQLCipher, three OS backends) silently undermines everything. None of the supply-chain tooling appears anywhere in the plan; this is the cheapest high-leverage gap to close and matches the open-source-trust positioning._
71. **Reproducible builds + signed SBOM + Sigstore provenance for every release** `[L]` - Produce a deterministic build (pinned toolchain, locked deps, normalized timestamps), emit a CycloneDX SBOM, and sign artifacts with Sigstore/cosign keyless provenance so users and corporate security teams can independently verify a downloaded binary matches the public source. _Value: The recommendation calls out that Ditto got banned by a security team and that new entrants must signal they'll still exist and be trustworthy; reproducible + provenance-attested builds are exactly what unblocks enterprise adoption and proves no backdoor was injected post-source. The plan covers code-signing/notarization for OS gatekeepers but nothing for source-to-binary integrity._
72. **#![forbid(unsafe_code)] boundary + fuzzing the untrusted-input parsers** `[M]` - Enforce forbid(unsafe_code) on the pure crates (vbuff-types/core/store), isolate all required unsafe to the platform crate behind audited wrappers, and stand up cargo-fuzz/AFL targets for the externally-controlled parsers (CF_HTML header parse, image decode, INCR reassembly, FTS query) plus an optional Miri lane. _Value: Every parser listed handles attacker-controlled clipboard bytes from any app on the system, the classic memory-safety attack surface; the test plan covers crypto round-trips and detectors but never fuzzes these. Fencing unsafe and fuzzing the input edges is what actually prevents the sandbox-escape scenario from being reachable in the first place._
73. **Per-collection encrypted vaults with independent locks and sync exclusion** `[L]` - Allow a user to mark a collection as a vault sealed under its own subkey (HKDF from the DEK or a separate Argon2id passphrase) that stays locked while the main history is unlocked, is masked in search until opened, and is sync-excluded by default. _Value: Master-password mode is all-or-nothing: unlocking to grab a normal snippet exposes the whole history. A per-vault lock lets users keep API keys / recovery codes in a compartment that's encrypted-within-encrypted and only opened deliberately, which the recommendation already flags as a wanted v2 feature but with no design._
74. **Capability-honest security posture badge with fail-closed enforcement** `[M]` - Compute a live per-platform security posture (is the keychain real or the encrypted-file fallback, is mlock active, is the sandbox engaged, can we see the foreground app on Wayland, is swap encrypted) and show it as a badge, with a 'strict mode' that fails closed (refuses to capture) whenever a required protection is unavailable. _Value: The architecture repeatedly warns about silent degradation (Wayland hiding app identity, Secret-Service-absent fallback, encryption-not-engaged) lulling users into false safety. Surfacing the real posture and letting paranoid users enforce it turns those documented soft-failures into explicit, user-controlled decisions instead of invisible downgrades._

### Platform backends (macOS/Windows/Linux/Wayland)

75. **Negotiate Wayland capture as ext -> wlr -> degraded** `[M]` - Prefer staging `ext-data-control-v1` where the compositor advertises it (confirmed in current KWin 6.4+ and Sway 1.11/wlroots 0.19 paths), then fall back to deprecated `wlr-data-control-unstable-v1`. Mutter and some other compositors advertise neither. _Value: registry truth plus a published compositor/version matrix prevents a generic Wayland claim from masking incompatible sessions._
76. **Use the RemoteDesktop/EIS path for portal-mediated Wayland paste** `[L]` - Model paste injection through the xdg-desktop-portal RemoteDesktop session and its EIS connection; do not treat InputCapture as a generic injection sender. Keep `wtype`/`ydotool` only as explicit opt-in fallbacks because compositor support and `/dev/uinput` privilege differ. _Value: the state machine must match the portal contract or degrade copy-only instead of producing prompts, privilege surprises, or wrong-target input._
77. **Persist only portal state the specific interface defines** `[M]` - Rebind GlobalShortcuts through `ListShortcuts`; for RemoteDesktop, store the returned restore token securely and replace it whenever a restored session returns a new one. Do not invent a GlobalShortcuts restore token. _Value: interface-specific persistence avoids repeated prompts without relying on fields the portal never defined._
78. **GNOME-Wayland companion shell extension to close the Mutter monitoring gap** `[L]` - Ship an optional GNOME Shell extension that watches the clipboard inside the compositor process and pipes change events to the vbuff daemon over the existing D-Bus interface, replacing the architecture's capture-on-summon degradation under Mutter. _Value: Mutter implements neither wlr- nor ext-data-control, so on stock GNOME vbuff silently misses copies (a documented limitation); the extension is the only sanctioned way to get true background capture there, and it reuses the D-Bus surface from feature 458._
79. **AllowSetForegroundWindow handshake for reliable Windows focus restore** `[M]` - Before paste-back on Windows, call AllowSetForegroundWindow(ASFW_ANY) / attach to the target thread's input queue (AttachThreadInput) so SetForegroundWindow actually raises the previous app instead of flashing the taskbar. _Value: Windows' foreground-lock policy makes a naive SetForegroundWindow fail when vbuff isn't the active foreground process, which is exactly the paste-back situation (feature 509); this is the standard workaround and prevents the wrong-window / silent-no-op paste that mistake #17 calls out._
80. **Hardware-backed key wrapping (Secure Enclave / TPM / TPM-bound DPAPI)** `[L]` - Wrap the SQLCipher DB key with a non-exportable hardware key: Secure Enclave via SecKeyCreateRandomKey on macOS, a TPM 2.0 persistent handle (tpm2-tss) on Linux, and CNG/NCRYPT machine-or-TPM key on Windows, instead of only storing the raw key in the keychain. _Value: Features 534-536 store the key in software secret stores, which a logged-in attacker or malware can read; hardware wrapping means a stolen disk or even a copied keychain is useless without the physical chip, directly strengthening the 'never leaked' positioning._
81. **Optional biometric/Touch ID gate before decrypting history** `[M]` - Require LocalAuthentication (Touch ID/Apple Watch) on macOS, Windows Hello (KeyCredentialManager) on Windows, and polkit/fprintd on Linux to unlock the encrypted history store or reveal concealed clips on demand. _Value: Adds a presence check beyond the at-rest key so an unlocked-but-unattended machine can't be raided for passwords and tokens in history; pairs naturally with the hardware-key wrapping and is a concrete privacy differentiator over Maccy/Ditto._
82. **Wayland/X11 connection-loss and compositor-restart recovery** `[M]` - Detect EOF/disconnect on the Wayland or X11 socket (compositor crash, restart, TTY switch) in the watcher thread and transparently re-establish the registry binding, re-grab selections, and re-register portal sessions with exponential backoff. _Value: Long-lived clipboard daemons routinely outlive a compositor restart or `gnome-shell --replace`; without reconnection the watcher silently dies and capture stops with no error, which no catalog feature addresses and which is a frequent real-world failure for X11/Wayland managers._
83. **Session/lock/fast-user-switching state machine across all platforms** `[M]` - Subscribe to session lock and user-switch signals (logind PrepareForSleep/Lock + loginctl seat, NSWorkspace screensaver/session notifications, Windows WTS_SESSION_LOCK/WM_WTSSESSION_CHANGE) and suspend capture, paste, and sync while locked or in a background session. _Value: Capturing clips while the screen is locked or while another user owns the seat is both a privacy leak and a source of cross-session contamination; explicit lifecycle handling prevents recording the lock-screen password field and stops paste-back firing into the wrong user's apps._
84. **Remote-session and multi-seat capability detection (RDP/VNC/SSH-X11/headless)** `[M]` - Detect when the session is remote or seatless (Windows GetSystemMetrics SM_REMOTESESSION, $SSH_CONNECTION + missing seat0, XWayland-over-VNC) and adapt: disable keystroke injection, prefer set-and-let-user-paste, and badge the capability matrix accordingly. _Value: Synthetic input and selection ownership behave very differently over RDP/VNC and on headless CI/servers where the CLI (feature 506) still wants to run; honest per-session capability detection (extending feature 544) avoids confusing silent failures in remote and automated environments._
85. **Adaptive macOS poll throttling tied to App Nap, battery, and display sleep** `[M]` - Drive the NSPasteboard changeCount poll interval from real signals (NSProcessInfo thermal/low-power state, ioreg battery, CGDisplayIsAsleep, and App Nap activity assertions) rather than the fixed 150-250 ms timer, and assert a QoS-utility activity only while a copy is in flight. _Value: The architecture mentions idle backoff but not the OS power signals that make polling battery-safe; coupling to App Nap and Low Power Mode keeps macOS from throttling the timer unpredictably and earns the 'near-zero idle CPU' claim on laptops._
86. **Per-monitor work-area popup placement with mixed-DPI cursor mapping** `[M]` - Compute popup origin from the cursor's physical position mapped through each display's scale factor and usable work area (excluding notch/menu-bar, taskbar, panels, docks), so the window opens fully on-screen on the cursor's monitor under fractional and mixed DPI. _Value: Features 384/546 cover HiDPI crispness and cursor-monitor placement separately but not the hard part: a cursor on a 200% monitor next to a 100% monitor needs correct logical-to-physical conversion or the popup spawns half off-screen or on the wrong display, a common multi-monitor bug._
87. **Hardened systemd user service + XDG autostart with display-server readiness gating** `[M]` - Provide a systemd --user unit (with NoNewPrivileges, ProtectHome, restart-on-failure, After=graphical-session.target) plus an XDG .desktop autostart that waits for WAYLAND_DISPLAY/DISPLAY to be live before binding backends. _Value: Feature 543's autostart is a bare XDG desktop entry; a sandboxed systemd user service gives crash recovery, journald logging, and reduced attack surface, and gating on display-server readiness fixes the race where autostart fires before the compositor socket exists and capture silently never starts._
88. **Klipper/GPaste/Win+V coexistence detection and conflict guard** `[S]` - On startup, detect a running native clipboard manager (KDE Klipper via D-Bus name org.kde.klipper, GPaste, or Windows Clipboard History enabled) and offer to disable it or enter a non-owning observe mode to prevent two managers fighting over selection ownership. _Value: Two clipboard managers both taking CLIPBOARD ownership on X11/Plasma causes capture loops, lost clips, and paste flicker (a real Linux pain point); proactive detection turns a baffling conflict into a one-click resolution, going beyond feature 526's Win+V-only coexistence note._
89. **Held-modifier sanitization before synthetic paste on every backend** `[S]` - Before injecting the paste combo, query and release any physically-held modifiers (CGEventSourceKeyState on macOS, GetAsyncKeyState on Windows, XQueryKeymap on X11) so a still-pressed hotkey modifier doesn't corrupt the Ctrl/Cmd+V into Ctrl+Shift+V or a shortcut. _Value: The global hotkey's own modifiers are often still down at paste time, leaking into the synthetic keystroke and either breaking the paste or triggering an unintended shortcut in the target app; mistake #17 flags leaked modifiers but no catalog feature specifies the cross-platform pre-injection cleanup._

### Extensibility, scripting & API

90. **Native subprocess plugins with capability-scoped pipe interfaces** `[L]` - Define a stable, bounded protocol for signed native plugin executables launched in an OS sandbox, communicating only over inherited local pipes and receiving exactly the capabilities their manifest grants. _Value: Solves the extensibility versus resident-process safety tension without adding a browser or WASM runtime. Plugin crashes and memory faults stay outside the clipboard process, while host-side timeouts, per-action consent, network/file scopes, and frame limits preserve least privilege._
91. **Per-plugin capability manifest with a one-time consent prompt** `[M]` - Each plugin ships a signed TOML manifest declaring exactly which capabilities it needs (clipboard-read, history-write, network hosts, filesystem paths, named-slot access); on install vbuff shows a plain-language permission sheet and the daemon enforces the grant at the host-call boundary, never trusting the plugin's own claims. _Value: Extensibility on a tool sitting over the user's most sensitive data needs the same trust story as the privacy bet (Bet 3). A least-privilege, user-auditable capability gate means installing a 'JSON formatter' plugin can be proven to have no network or full-history access - turning the extension store (#450) from a footgun into a selling point. Reuses the SAS/consent UX patterns already in the design._
92. **Transform pipeline as a typed content-kind algebra** `[M]` - Model each transform as a pure op with declared input/output ContentKind (text->text, image->text via the OCR plugin, json->json), so the pipeline editor only offers ops whose input type matches the current stage and rejects ill-typed chains (e.g. 'sort lines' after 'render QR') at edit time instead of at run time. _Value: The catalog has a transform chain (#289) but as an untyped list. Typing the pipeline against the existing ContentKind enum turns it into a composable, validated graph: the GUI can gray out inapplicable ops, the CLI can statically check a pipeline file, and plugins slot in as new typed ops. This is what makes 'power as GUI actions, not a DSL' (the recommendation's stance) actually usable for chains._
93. **Deterministic dry-run / preview op in the IPC protocol** `[S]` - Add a daemon RPC that applies a transform pipeline to a clip and returns the result plus a per-stage diff and timing WITHOUT writing anything, sourcing the same code path the real paste uses, so GUI preview (#291), CLI --dry-run, and plugin self-tests all share one honest preview. _Value: Catalog #291 lists 'preview transform result' as a v2 GUI feature; promoting it to a first-class protocol verb means every surface (popup, CLI, future Raycast/Alfred extension) gets identical preview semantics for free and plugin authors can write golden-output tests against it. Cheap given transforms are already pure and paste-time-only by design._
94. **Subscription filters in the event stream (server-side predicates)** `[M]` - Extend `vbuff watch` (#455) so a client can register a filter expression - content-kind, source-app, regex, tag, size range - and the daemon only emits matching events, instead of every consumer receiving the full firehose and filtering client-side. _Value: A webhook (#452) or automation script that only cares about copied URLs shouldn't decrypt and ship every password-manager clip across the socket to discard it. Server-side filtering reduces the data-exposure surface of the whole integration layer and cuts wakeups for battery-sensitive scripts. Composes naturally with the FTS5/predicate logic already in vbuff-core._
95. **Versioned, capability-negotiated IPC handshake** `[M]` - Make the first frame on every UDS/named-pipe connection a Hello exchange carrying protocol version, client identity, and a requested capability set, with the daemon replying with the granted set; unknown future verbs degrade gracefully instead of breaking older CLIs against a newer daemon. _Value: The architecture pins a single binary serving CLI, pickers, URL handler, and future plugins over one socket; without explicit version negotiation, a user with an updated daemon and a stale `vbuff` in PATH (common with Homebrew/AUR) hits silent protocol mismatches. A negotiated handshake also lets the daemon hand local scripts a narrower capability set than the GUI - foundational for everything else in this theme._
96. **Source-app-scoped capability tokens for the local HTTP API** `[M]` - When the inbound localhost API (#453) is enabled, issue per-client bearer tokens that are scoped (read-only, specific tags, no secret clips) and revocable from settings, rather than a single all-or-nothing key, with each token's last-use and origin shown in an access log. _Value: A browser-extension bridge (#468) and a home-automation script have very different trust levels; one shared key means compromising the weakest integration exposes the entire history. Scoped, individually-revocable tokens with an audit trail mirror the device audit/revoke story already planned for sync (#323) and keep the 'never leaked' promise intact when local automation is in play._
97. **Import/export adapter SDK as a distinct plug point** `[M]` - Define a small ImportAdapter/ExportAdapter trait (parse a foreign format into Clip/Flavor; serialize the other way) so importers for Ditto/CopyQ/Maccy (#462), text-expander libraries (#463), and new formats are plugins rather than hardcoded match arms, with a `vbuff import --adapter <name> <file>` CLI verb. _Value: The catalog lists a fixed set of importers; every clipboard manager that appears later means a core code change. A pluggable adapter point lets the community contribute migrators (the abandonment graveyard means new users are constantly migrating FROM dead tools) without a vbuff release, and the same trait powers round-trip export for backup tooling._
98. **Content recognizer plugins that emit typed actions** `[M]` - Let plugins register pattern recognizers (e.g. 'this is a JWT', 'an IBAN', 'a git SHA', 'a tracking-laden URL') that run on capture metadata only and attach suggested one-click actions to the clip's quick-action menu, without the plugin ever needing to see unrelated clips. _Value: vbuff already plans on-copy type detection (URL/email/code/color/path) to drive icons and contextual actions; exposing that detection as an extension point means domain-specific users (developers, finance, ops) get tailored actions ('decode this JWT', 'strip UTM params') as installable recognizers. It is the highest-leverage, lowest-risk plugin surface because recognizers touch only the single active clip._
99. **Signed, reproducible native plugin bundles with a lockfile** `[M]` - Distribute plugins as platform-targeted signed bundles (manifest + executable + assets) verified against a publisher key on install, and record installed plugins with pinned versions, target, and hashes in a `plugins.lock` next to the versionable config (#471). _Value: Signature verification stops tampered executables from impersonating a trusted plugin, while explicit platform targets and lockfile hashes keep configuration portable and auditable without loading third-party code into vbuff._
100. **Transactional batch RPC for atomic multi-clip operations** `[S]` - Add a daemon verb that executes an ordered list of mutations (add N clips, tag, pin, delete) as one transaction against the single SQLite writer, returning all-or-nothing, so a CLI import or an automation script never leaves history half-mutated on partial failure. _Value: The store funnels all writes through one actor; a batch verb maps cleanly onto a single SQLite transaction and gives scripts/importers the atomicity the per-call API can't. It directly supports the adapter SDK (import 500 clips atomically) and named-slot staging workflows (#474), and prevents the partial-state corruption that plagues file-based competitors._
101. **URL-scheme action tokens for x-callback-style return values** `[S]` - Extend the `vbuff://` scheme (#444) with an x-callback-url style convention - `vbuff://transform?op=base64&x-success=<caller-url>` - so a launcher, note app, or Shortcuts/Automator flow can invoke a transform and receive the result back via a callback URL instead of only firing a one-way intent. _Value: The catalogued URL scheme is fire-and-forget; an x-callback contract makes vbuff a composable node in macOS Shortcuts, iOS, and note-app automation chains (the same pattern Bear/Things popularized) without needing the full CLI present. It bridges the gap between the GUI-only casual user and the CLI power user for the large 'I automate with Shortcuts' middle segment._

### Testing, CI & release engineering

102. **Coverage-guided fuzz corpus for the flavor deserializer and CF_HTML parser** `[M]` - Add cargo-fuzz/libFuzzer targets (distinct from the existing proptest suite) for serde_clip flavor decode, the CF_HTML header parser, and UTI/CF/MIME format mapping, seeded from a checked-in corpus of real-world clipboard payloads and wired to OSS-Fuzz for continuous fuzzing. _Value: proptest is generation-based and shallow; coverage-guided fuzzing of the exact byte-parsing paths that ingest untrusted clipboard data from any app is where a real crash/panic/UB lives. Free continuous fuzzing via OSS-Fuzz catches regressions between releases for a security-positioned tool._
103. **Golden-image GUI snapshot tests for the popup across themes and DPI** `[M]` - Upgrade the egui_kittest 'smoke test' to deterministic golden-PNG snapshots of the virtualized list, type icons, match-highlighting, and empty/capability-badge states, rendered with a pinned font and fixed seed at 1x/1.5x/2x scale and dark/light, diffed in CI with a perceptual tolerance. _Value: Polish is Bet 1 and egui 'draws its own widgets', so visual regressions (icon drift, highlight misalignment, clipped RTL galleys, broken DPI) are invisible to logic tests. Golden images make the headline polish claim regression-proof and catch cosmic-text galley breakage on CJK/RTL content._
104. **Reproducible-build verification job that rebuilds and bit-compares the release** `[L]` - A CI job that builds each release artifact twice in independent clean containers (pinned Rust toolchain, --remap-path-prefix, SOURCE_DATE_EPOCH, locked Cargo.lock) and fails if the two binaries are not byte-identical, publishing the expected hashes alongside the release. _Value: Bet 3 is 'private by construction'; a clipboard manager handling secrets must let security teams independently verify the shipped binary matches the source. Reproducibility is also the prerequisite for any future binary-transparency or third-party rebuild attestation._
105. **Signed, staged auto-update with key-rotation and downgrade-protection tests** `[L]` - Build the v1 updater on a signed manifest (minisign/TUF-style) with monotonic version pinning, a CI test matrix that asserts the client rejects tampered manifests, replayed/older versions, and wrong-key signatures, plus a percentage-based staged-rollout gate keyed off opt-in crash telemetry. _Value: An auto-updater is the single most dangerous attack surface in a privacy tool: a compromised update channel pushes malware to every machine. Testing rejection of tamper/rollback/wrong-key, and staging rollouts behind crash rates, turns 'check for updates' (feature #487) into something safe to ship._
106. **Multi-compositor Wayland CI matrix beyond sway (GNOME Mutter, KDE KWin)** `[L]` - Extend the headless-sway job to run the capability probe and capture/paste conformance suite under nested Mutter and KWin, asserting `ext-data-control-v1`, legacy `wlr-data-control`, or the documented degraded mode exactly as advertised. _Value: sway-only CI tests one favorable protocol path. Testing both supported and unsupported compositors catches false capability badges and silent loss before release._
107. **Paste-injection fidelity harness with a sink app and modifier-leak assertions** `[M]` - A per-OS integration test that focuses a tiny headless 'sink' window, drives the real PasteBackend (CGEvent/SendInput/XTEST), and asserts the received text equals the source byte-for-byte with no stray modifiers, covering the Ditto 'literal v pasted' and held-modifier-leak bugs end-to-end. _Value: Paste-back correctness is currently only covered by a ClipboardOnly-fallback unit test; the actual keystroke-injection path (where competitors' worst bugs live: wrong char, leaked Ctrl, wrong target) has no automated check. A real sink closes the loop on 'a picker that does not paste back is not a clipboard manager'._
108. **Packaging smoke tests: install the built artifact in a clean OS image and launch it** `[M]` - Post-build CI stage that takes the actual .dmg/.pkg, MSI/MSIX, and .deb/.rpm/Flatpak/AppImage, installs each into a pristine VM/container with no Rust toolchain, launches vbuff headless, and asserts the single-instance socket binds, the tray registers, and version reports correctly. _Value: Native installers (feature #550) routinely break on missing runtime libs, bad desktop files, unsigned binaries, or wrong entitlements - failures that never surface in `cargo build`. A clean-room install-and-launch gate per format prevents shipping an installer that no fresh machine can actually run._
109. **macOS notarization + Gatekeeper assessment as a release gate** `[M]` - A CI step that codesigns with hardened runtime and the global-hotkey/Accessibility entitlements, submits to Apple notarization, staples, then runs `spctl --assess` and `codesign --verify --deep --strict` against the stapled .dmg, blocking release on any failure. _Value: Hardened-runtime notarized packaging (feature #549) is a yes/no gate users hit on first launch; an unstapled or mis-entitled build silently fails Gatekeeper and breaks the global hotkey. Asserting notarization + spctl in CI catches it before users do, not in a bug report._
110. **Mutation testing on vbuff-core to score the crown-jewel test suite** `[M]` - Run cargo-mutants over vbuff-core (dedup, eviction, capture gate, classify, hash) in a scheduled CI job, treating surviving mutants in the fail-closed gate and pin-exemption logic as must-fix, with a tracked mutation-kill-rate threshold for the privacy-critical modules. _Value: proptest invariants can pass while still missing logic gaps; mutation testing measures whether the tests would actually catch a bug introduced into the gate or eviction. For modules where a single wrong branch means a captured secret or an evicted pin, kill-rate is a far more honest quality signal than line coverage._
111. **Supply-chain gate: cargo-deny, cargo-vet, and SBOM + provenance on every release** `[M]` - CI enforcement of cargo-deny (license/advisory/duplicate bans) and cargo-vet (audited dependency trust) on every PR, plus generation of a CycloneDX SBOM and SLSA build-provenance attestation attached to each GitHub release artifact. _Value: vbuff depends on a wide native crate surface (objc2, windows-rs, SQLCipher, tray-icon, global-hotkey) flagged as a maturity risk; a malicious or vulnerable transitive dep undermines the entire privacy story. Vetting + SBOM + provenance gives security teams the audit trail they need to approve a tool that touches the clipboard._
112. **Deterministic time/clock seam so capture, eviction, and Lamport tests are reproducible** `[S]` - Introduce a Clock trait (alongside the existing FakeStore/FakeClipboard fakes) injected into core so retention windows, self-destruct timers, and Lamport-clock sync ordering are driven by a controllable fake clock, eliminating wall-clock flakiness from time-dependent tests. _Value: Retention, per-clip expiry (#362), scheduled clearing, and last-writer-wins conflict resolution are all time-sensitive; testing them against real time is flaky and slow. A clock seam makes 'item expires after N minutes' and 'concurrent-edit LWW' assertions exact and instant, and is a prerequisite for trustworthy v2 sync conflict tests._
113. **Capture-observability benchmark as a hard CI assertion** `[M]` - Fire N distinct clipboard edges as fast as each OS contract permits. Event-driven backends must store every generated edge exactly once within a latency budget. Polling backends must store every observed state, flag every detectable sequence jump, and never invent recovery of an overwritten payload. _Value: this blocks silent loss and false success claims while respecting the information each native API can actually provide._
