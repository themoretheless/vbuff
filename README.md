# vbuff

**One clipboard, every machine. Never lost, never leaked.**

A fast, private, cross-platform clipboard manager written in Rust. The current build captures text and images into a durable, searchable local SQLite history, summons a keyboard-driven popup with a global hotkey, and pastes the chosen clip back into the app you were using. SQLCipher at-rest encryption and live peer-to-peer transport remain release gates, so this repository does not yet claim encrypted local storage or working cross-device sync.

---

## What is vbuff, and why it exists

Clipboard managers are a mature but deeply fragmented category, and no single product covers the things that matter at once. The best tool on each platform is usually *only* on that platform: **Ditto** is the de facto free manager on Windows but is Windows-only and local-only; **Paste** is beautifully polished and syncs across Apple devices but is macOS/iOS-only and subscription-only; **CopyQ** genuinely spans macOS, Windows and Linux but wears a dated, scripting-heavy UI and has no real sync; **CrossPaste** reaches every platform and syncs privately but only over the LAN and without the polish. A person who works across macOS, Windows and Linux cannot carry one mental model, one keybinding scheme, or one private history across all three.

That is the four-corner gap vbuff is built to close: **be truly cross-platform (macOS + Windows + Linux), genuinely polished, privately synced, and approachable, all at the same time.** Every competitor wins at most two of those corners; vbuff aims to occupy all four. The non-negotiable target is privacy: local-only by default, encrypted at rest, OS "do not store" hints evaluated before persistence, and every rejection routed through one fail-closed gate. The current fallback backend cannot yet observe all native privacy hints, and the current SQLite file is not encrypted; both limitations are tracked explicitly rather than hidden behind the target language.

---

## Target platform support

vbuff is designed as one codebase with native, per-OS backends behind common Rust traits. The table below is the target backend matrix. Today the executable uses the `arboard` fallback for text/raw-RGBA clipboard access on all platforms; it does not yet enumerate every flavor, provide native generation/provenance, or expose concealed markers.

| Platform | Clipboard capture | Global hotkey | Paste-back | Notes |
|---|---|---|---|---|
| **macOS** | `NSPasteboard` `changeCount` polling (~150-250 ms, adaptive idle backoff) | Carbon `RegisterEventHotKey` | Focus restore + synthetic Cmd+V | Paste-back needs **Accessibility** permission (granted in System Settings). Honors `org.nspasteboard.ConcealedType` / `TransientType`. |
| **Windows** | `AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE` (event-driven) | Win32 `RegisterHotKey` | `SetForegroundWindow` + `SendInput` Ctrl+V | Honors `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory`. |
| **Linux / X11** | `CLIPBOARD` selection ownership via XFIXES selection events | `XGrabKey` | `XTEST` Ctrl+V | vbuff takes selection ownership so clips survive the source app closing. |
| **Linux / Wayland** | `wlr-data-control` (wlroots: Sway, Hyprland, river; also KDE Plasma) | `GlobalShortcuts` portal (`xdg-desktop-portal`) | virtual-keyboard / `wtype` / `ydotool`, else set-and-let-user-paste | See the GNOME caveat below. |

> **GNOME on Wayland caveat.** GNOME's Mutter compositor does not implement `wlr-data-control` and offers no sanctioned background clipboard-monitor API. On GNOME-Wayland, vbuff degrades gracefully to **capture-on-summon** (it reads the clipboard when the popup opens, since the popup has focus) plus a manual capture hotkey, and it shows an honest in-app explanation of what is and is not being captured rather than silently dropping copies. This is a genuine platform limitation, not a bug. X11 sessions and wlroots/KDE Wayland sessions are unaffected.

Source-app attribution and per-app exclusion rely on knowing the foreground app, which Wayland intentionally hides from clients. On Wayland those features are best-effort and clearly badged as unavailable where the compositor cannot provide the information; content-pattern and secret-detection rules still apply.

---

## Product target

Curated from the project's feature catalog (the strongest MVP and v1 items, not the full 640). This section describes the intended product; the authoritative current implementation is listed under **Status** and in the batch ledger below.

### Capture everything, byte-for-byte

- Background watcher captures **every** clipboard change automatically, idling near 0% CPU.
- Stores **plain text, rich text/HTML, RTF and images** out of the box; files/folders, custom MIME types and color clips follow in v1/v2.
- **Captures all flavors of a single copy atomically** (a web copy keeps HTML + plain text + image together) so you choose the representation at paste time.
- **Byte-for-byte fidelity**: no whitespace, newline or encoding normalization, so editor selections and exact payloads round-trip.
- **Deduplicates** re-copied content (move-to-top instead of a duplicate row) via a BLAKE3 content hash.
- **Pause / resume**, **incognito mode**, and a **manual capture-on-demand** hotkey.

### Instant keyboard-driven recall

- Global hotkey opens a popup near the cursor that **filters as you type**, with live match highlighting.
- Fully keyboard-driven: navigate, filter, paste-back and pin without touching the mouse.
- **Number-key quick-pick** for the top items, and per-action shortcuts (paste, paste-plain, pin, delete).
- Type / pinned / favorite filters, empty and no-results states, dark/light themes, image thumbnails, per-type icons.
- Virtualized list keeps search-as-you-type fluid over very large histories; FTS5 indexed search is enabled in v1 for 100k+ items.

### Reliable paste-back

- Restores focus to the previously active app and injects a real paste, so the clip lands where you were typing.
- **Plain vs keep-formatting** paste, with a one-shot paste-as-plain action.
- **Enter** to paste-back, double-click to paste, or a number key for quick-pick; **copy-only** fallback when paste injection is unavailable.
- Self-write suppression so vbuff never re-captures its own paste.

### Organization that survives restarts

- **Pin to top** and **star as favorite**; pinned items are exempt from eviction and persist across restarts as a reusable snippet bank.
- Promote a clip to a **permanent** item that never auto-prunes.
- Tags, folders/collections, named tabs, pinboards, notes, color labels and manual drag-reorder arrive in v1.
- Configurable retention: count cap, total-size cap, time expiry, or unlimited mode (pins/favorites always exempt).

### Snippets and quick transforms (growing through v1)

- Saved snippets with abbreviation expansion, insert-by-hotkey, folders and a built-in editor; date/time placeholders in the MVP set.
- Promote any clip into a snippet in one keystroke.
- Quick-action palette with change-case, trim whitespace, strip formatting and literal find-and-replace; programmer-case, regex replace, base64/URL encode-decode and JSON pretty-print expand the set in v1.
- One product instead of a separate clipboard manager *and* a separate text expander.

### Private and trustworthy by construction

- **Target:** encrypted at rest with the key held in the OS secret store, not beside the database.
- **Honors OS concealed/secure markers** and password-field hints, skips designated apps, supports regex/keyword exclusion rules and built-in secret detection.
- **Local by default, zero telemetry, no network calls** out of the box.
- Auto-clear-on-timer, wipe-on-demand, and shorter retention for sensitive clips.
- Cross-device, end-to-end encrypted sync is planned and opt-in (v1 foundation, v2 breadth), never the default path and never a backend that can read your data.

---

## Target privacy and security

vbuff is designed around a single hard rule: **fail closed.** Every uncertainty in "should we capture this?" should resolve to *do not capture*, and the decision must run before durable persistence. The current repository now has one ordered gate, source rules, sensitive/local-only metadata, masked sensitive rows, OTP TTL, byte-free skip recovery, self-write suppression, content-free loss accounting, Unix owner-only paths, SQLite secure deletion, WAL truncation, and redacted diagnostics. Native concealed/transient hints, a broader secret-detector corpus, SQLCipher with OS-keystore keys, cross-platform residue verification, and live opt-in E2E sync are still required before the full privacy claim is earned. The threat model never claims protection from root/admin, an attached debugger, or a kernel attacker.

---

## Status

vbuff is in active early development. The current executable uses `arboard`, `global-hotkey`, `egui`, and `enigo` for the resident copy -> store -> popup -> paste loop. SQLite schema v6 includes adaptive prose/code FTS5 search, exact/near fingerprints, a dedup reuse ledger, transactional CAS, verified migrations, per-kind retention rules, session protection, encrypted grace-bin primitives, expiry/clawback, and fail-closed local embeddings. Capture adds adaptive cadence, restart supervision, byte/RSS pressure policy, structural-secret detection, typed health/size alerts, remote-session startup pause, redacted metrics, an external heartbeat, and a live hash-chained privacy ledger. The popup has compact History, Compose, Trust, and Settings surfaces with a menu-bar action icon, command palette, sticky kind/source filters, default profiles, an optional metadata-only health digest, stale-pin review, plain-text cloning, calendar expiry labels, adaptive density, large preview, accessibility live announcements, sensitive peek, and short Undo. Config schema 2 upgrades require explicit `migrate preview/apply` and preserve an owner-only rollback copy. Durable recovery is not wired until an OS-keystore provider supplies its key. `vbuff doctor`, config handoff, `vbuff ask`, and offline artifact verification are active. A fake-data WASM popup compiles and was browser-reviewed at desktop/compact sizes, but is not hosted and cannot access real clipboard data. IPC/plugin/sync/update contracts, including device UX and browser/editor/automation/MCP/launcher/terminal/webhook boundaries, remain tested foundations, not live network, extension, replication, or update-install features. The exact gaps and workarounds live in the [public limitation ledger](docs/limitations.md). Native all-flavor backends, SQLCipher, per-OS conformance, daemon extraction, the full history CLI, and live sync remain target work.

---

## Architecture at a glance

vbuff is a Cargo **workspace** with a fat, OS-agnostic core and thin platform crates. The cardinal rule: `vbuff-core` contains zero OS-specific code and zero GUI code, so the bulk of the logic is unit- and property-testable on any host with mock backends.

| Crate | Role | In MVP? |
|---|---|---|
| `vbuff-types` | Plain shared clip, status, notice, and minimal IPC contracts; serde only | Yes |
| `vbuff-core` | Pure dedup/eviction/classification plus capture, composition, everyday workflow, privacy/AI, embedding, delivery, feedback, and observability policy | Yes (partial) |
| `vbuff-store` | Bundled SQLite schema v6, FTS5, migrations, sharded CAS, exact/near dedup lifecycle, externally keyed recovery primitives, eligible local embeddings, expiry, and audits; SQLCipher/keystore wiring remains a release gate | Yes (partial) |
| `vbuff-platform` | Current trait layer plus `arboard` / `global-hotkey` / `enigo` adapters; target adds native per-OS clipboard, hotkey, paste and tray impls | Yes (partial) |
| `vbuff-gui` | Current `eframe` History/Trust/Compose/Settings popup plus a fake-data WASM demo target; native assistive-technology evidence remains | Yes (partial) |
| *(root app)* | `src/main.rs` composes startup; focused modules own capture supervision, history, commands, paste timing, event-loop wiring, autostart, tray/menu-bar integration, and minimal single-instance handoff | Yes |
| `vbuff-daemon` | Background wiring, IPC server, single-instance guard (as the model splits out) | Later |
| `vbuff-ipc` | Tested handshake, filtered events, scoped tokens, batches, and bounded browser/editor/Vim/automation/MCP/launcher/terminal/webhook contracts; no live daemon dispatch yet | Foundation only |
| `vbuff-plugin` | Tested WIT/consent/typed-plugin contracts, bounded import/export adapters, and four curated recipes; no Wasmtime host or install gallery yet | Foundation only |
| `vbuff-sync` | Protocol/crypto plus bounded device trust, rehearsal, replay, outbox, retention, travel, handoff, and approval policy; no discovery, transport, persistence, or replication | Foundation only |
| `vbuff-update` | Signed manifests, key rotation, downgrade/replay defense, staged rollout, build attestation, and streaming checksum verification | Foundation + verifier CLI |
| `vbuff-cli` | `vbuff` verbs as a pure IPC client | Later |

The GUI is **egui** rendered via **eframe**. Immediate mode is a natural fit for a search-as-you-type list: each keystroke re-filters the rows with no retained widget tree to diff, and `ScrollArea::show_rows` gives row virtualization for free. The current store is bundled **SQLite** via `rusqlite`, with FTS5 and an out-of-row content-addressable blob store already active; SQLCipher and OS-keystore keying remain target work. Dedup uses **BLAKE3**. See [architecture.md](architecture.md) for the full target design and current cut lines.

---

## Read the project in small pieces

The repo is intentionally split so you can understand it without loading the whole product into your head at once:

1. **Data shapes and wire contracts:** start with `crates/vbuff-types/src/lib.rs`, then `status.rs` and `ipc.rs` (`Clip`, flavors, ids, `CaptureHealth`, redacted notices, and the minimal startup intents).
2. **Pure behavior:** read `crates/vbuff-core/src/*` for hashing, classification, filtering, eviction, and `compose.rs`, then one small `workflow/` module such as `everyday.rs`/`links.rs` or one `intelligence/` module. This crate stays OS-free and GUI-free.
3. **Persistence:** read `crates/vbuff-store/src/lib.rs`, then `search.rs`, `migration.rs`, `cas.rs`, and `lifecycle.rs` for schema/query ownership, verified upgrades, blob lifecycle, dedup/recovery, and retention.
4. **Platform ports:** read `crates/vbuff-platform/src/traits.rs` first; native per-OS backends should hang behind those traits.
5. **GUI state and rendering:** read `crates/vbuff-gui/src/state.rs`, then `design.rs`, `experience.rs`, `view.rs`, and finally `app.rs`.
6. **History boundary:** read `src/history.rs`; it is the only app-layer facade that couples store mutations to refreshed GUI snapshots.
7. **Resident workflows:** read `crates/vbuff-core/src/capture/` for pure decisions, then `src/capture.rs` for runtime supervision and `src/paste.rs` for clipboard-write-before-delayed-paste sequencing.
8. **Diagnostics publisher:** read `src/diagnostics.rs`; capture and command handling publish typed status through this narrow boundary instead of depending on GUI internals.
9. **Startup handoff:** read `src/single_instance/mod.rs` for framing/ownership, then `unix.rs` or `windows_fallback.rs` for one transport; this slice owns bind-or-forward, liveness, stale recovery, and cleanup.
10. **Shared commands and OS surfaces:** read `src/commands.rs`, then `src/tray.rs` and `src/autostart.rs`.
11. **Sync foundation:** read `crates/vbuff-sync/src/lib.rs`, then one concern at a time (`clock`, `crdt`, `crypto`, `membership`, `policy`, `merkle`, `ledger`, `capability`, `wire`). For device UX, start at the `device_experience.rs` facade and open only `policy.rs`, `outbox.rs`, `travel.rs`, or another focused submodule. It is intentionally not linked into the resident runtime yet.
12. **Composition shell:** read `src/app.rs`, then `src/main.rs` last. `app.rs` owns event-driven hotkey/tray/second-instance wakeups; `main.rs` only constructs and starts focused services. A duplicate launch forwards `ShowPopup` to the running instance.
13. **Reliability and security policy:** read `crates/vbuff-core/src/reliability.rs`, `secret.rs`, and `security_audit.rs`; then read `src/memory_pressure.rs`, `src/maintenance.rs`, and `src/doctor.rs` for the runtime adapters.
14. **Capability and lifecycle contracts:** read `crates/vbuff-platform/src/capabilities.rs`, `security.rs`, `lifecycle.rs`, `wayland.rs`, and `windows.rs`. These files describe honest fallback decisions; they are not native backend implementations.
15. **IPC and plugin foundations:** read `crates/vbuff-ipc/src/lib.rs`, then one file in `integration/`; read `crates/vbuff-plugin/src/lib.rs`, then `recipes.rs` or `adapter.rs`. Neither crate is connected to an ambient network listener or plugin runtime.
16. **Release trust:** read `crates/vbuff-update/src/lib.rs`, then `manifest.rs` and `attestation.rs`; `src/verify.rs` is the narrow offline CLI adapter.
17. **Delivery evidence:** read `crates/vbuff-core/src/delivery.rs`, `slo.rs`, and [decision-gates-151-200.md](docs/decision-gates-151-200.md); machine/human evidence remains separate from deterministic gate logic.
18. **Operations and honest claims:** read [limitations.md](docs/limitations.md), [maintainer-handoff.md](docs/maintainer-handoff.md), [scope-review.md](docs/scope-review.md), then `.github/workflows/release-provenance.yml`.

The SOLID/DRY rule of thumb is simple: data and serializable status/IPC contracts live in `vbuff-types`, pure logic is testable, platform code is behind traits, storage owns SQL, GUI owns presentation, `AppCommand` is the one command vocabulary, single-instance transport stays isolated, and `main.rs` only composes the pieces.

## Design direction

vbuff should feel like a quiet resident tool, not a marketing page or a scripting console. The first screen is the usable popup: dense enough for repeated work, calm enough for secrets, and fully keyboard-driven.

The current design baseline is implemented rather than aspirational: one token module controls stable popup and row dimensions; familiar actions are fixed icon buttons with tooltips; empty/search-empty states are distinct; and delete/clear require explicit confirmation. Popup and tray share one typed capture-health vocabulary, while content-free notices report command outcomes. History keeps privacy and status scannable with a restrained kind/source filter row, one clear affordance, calendar expiry, session-protection state, and a side preview; Trust holds detailed capability/SLO/release evidence; Settings contains review-before-apply profiles, the optional metadata-only digest, and stale-pin review; Compose keeps stack/form/merge work out of nested cards. Empty history offers two explicit local starter sets and remains empty if declined. Twenty-four checked-in golden images cover empty, populated, alerts, Trust, Compose, and Settings across theme and DPI. Hotkey, tray, and second-instance events wake the UI directly instead of forcing a 100 ms idle poll; a visible popup refreshes at one hertz rather than every frame.

- **Popup:** stable row dimensions, clear selected state, type icon, source/time metadata, and small state badges for pinned, sensitive, paused, degraded, local-only, and synced.
- **Actions:** repeated row tools should use icon buttons with tooltips; destructive actions such as clear history, wipe, revoke, or reset should use explicit text and confirmation.
- **Density:** compact and comfortable modes should share one layout system instead of ad hoc spacing.
- **Accessibility:** focus rings, high contrast, screen-reader labels, reduced motion, and pointer-free navigation are part of the core design, not a later cleanup.
- **Trust:** privacy state should be visible at decision points. A sensitive row should look protected and understandable, not alarming or hidden behind mystery UI.

---

## Implementation batches

The 600-point review is executed in batches of 50. Each batch gets an item-by-item disposition, three review passes, workspace tests, strict clippy, documentation synchronization, and its own commit before the next range starts.

| Batch | State | Evidence |
|---|---|---|
| 001-050 | Implemented/reviewed at runtime or foundation level; native and transport dependencies remain explicit | [Batch 001-050 ledger](docs/implementation-batch-001-050.md) |
| 051-100 | Implemented/reviewed at runtime, foundation, adapted, or native-required level | [Batch 051-100 ledger](docs/implementation-batch-051-100.md) |
| 101-150 | Implemented/reviewed with release, Trust UI, migration, sync, and policy boundaries explicit | [Batch 101-150 ledger](docs/implementation-batch-101-150.md) |
| 151-200 | Implemented/reviewed with Compose, privacy/AI, integration contracts, data freeze, and delivery gates explicit | [Batch 151-200 ledger](docs/implementation-batch-151-200.md) |
| 201-250 | Implemented/reviewed with power workflows, responsive/a11y UI, schema 6 lifecycle, and native/key-provider gates explicit | [Batch 201-250 ledger](docs/implementation-batch-201-250.md) |
| 251-300 | Implemented/reviewed with everyday runtime UX, device/integration foundations, and operational evidence boundaries explicit | [Batch 251-300 ledger](docs/implementation-batch-251-300.md) |
| 301-600 | Queued in sequential groups of 50 | Canonical range map below |

"Foundation" is not a synonym for shipped: the `vbuff-sync` algorithms compile and are tested, but the app still has no live sync transport; provenance and generation contracts exist, but `arboard` cannot populate native metadata. The ledger is the source of truth for those distinctions.

---

## 600-point review backlog

The 600 proposals, improvements, problems, bugs and "done badly" notes are kept as one numbered backlog, split by decision level so each file stays readable. Treat this as review input, not an automatic scope increase; `plan.md` decides what graduates into implementation. Items 501-600 are tied to the reviewed repositories, papers, and standards in [docs/repositories-research-100.md](docs/repositories-research-100.md).

| Range | Canonical file | Lens |
|---|---|---|
| 1-113 | [architecture.md](architecture.md) | Engineering architecture, backends, storage, privacy, sync, testability |
| 114-197 | [recommendation.md](recommendation.md) | Product strategy, differentiation, business model, roadmap tradeoffs |
| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) | User workflows, UI/UX, sync experience, integrations, operations |
| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) | Privacy controls, search, data model, platform fit, teams, automation |
| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) | Current implementation problems, SOLID/DRY slices, design fixes, review hygiene |
| 501-600 | [docs/ideas-501-600.md](docs/ideas-501-600.md) | Evidence-backed native correctness, text/search, security, local-first sync, verification |

The separately sourced post-600 candidates [601-610](docs/ideas-601-610.md) and [611-620](docs/ideas-611-620.md) are follow-up research input, not an expansion of the canonical 1-600 execution goal.

---

## Build from source

vbuff is a standard Cargo workspace. You need a recent stable **Rust toolchain** (install via [rustup](https://rustup.rs)) plus a few per-OS native dependencies.

### Prerequisites

**macOS**
- Xcode Command Line Tools: `xcode-select --install`
- A recent stable Rust toolchain. No extra packages are required to build; the current store uses bundled SQLite via `rusqlite`. SQLCipher encryption is planned.

**Windows**
- Rust with the MSVC toolchain (the default from rustup) and the **Visual Studio Build Tools** (the "Desktop development with C++" workload) for the C/C++ linker.
- No additional system libraries are required for the current bundled SQLite store.

**Linux (X11 and Wayland)**
- A C toolchain and `pkg-config`.
- X11 development headers (for X11 sessions): on Debian/Ubuntu, `libx11-dev`, `libxcb1-dev`, `libxfixes-dev`.
- Wayland and clipboard tooling (for Wayland sessions): `libwayland-dev`; `wl-clipboard` is recommended as a fallback path on compositors without `wlr-data-control`.
- GUI/runtime libraries for eframe: development packages for `libxkbcommon`, plus your GPU/GL stack. On Debian/Ubuntu: `libxkbcommon-dev`, `libgl1-mesa-dev`.
- The Linux build deliberately links both the X11 and Wayland client libraries so one binary runs under either session.

> Exact package names vary by distribution. The list above targets Debian/Ubuntu; translate accordingly for Fedora, Arch, etc.

### Clone, build and run

```sh
# 1. Clone
git clone https://github.com/your-org/vbuff.git
cd vbuff

# 2. Build the whole workspace, optimized
cargo build --release

# 3. Run the app (the single-process MVP binary)
cargo run --release
```

The optimized binary is written to `target/release/vbuff`. For day-to-day development, `cargo run` (debug) and `cargo test --workspace` are the usual loop. `cargo build --workspace` builds every crate, including the later-phase ones as they land.

---

## Usage

1. **Copy as normal.** vbuff captures clipboard changes in the background automatically.
2. **Open the popup with the global hotkey:**
   - macOS: **Cmd + Shift + V**
   - Windows / Linux: **Ctrl + Shift + V**
3. **Type to filter** the history; matches highlight as you go.
4. **Navigate** with **Up / Down** (Home / End jump to the ends of the list).
5. **Press Enter** to paste the selected clip back into the app you were just using.
6. **Number keys (1-9)** quick-pick the corresponding item directly.
7. **Pin** an item to keep it at the top and exempt it from eviction; **delete** removes it from history.
8. Add text clips to **Compose** to edit/reorder a temporary paste stack, name form slots, or merge items as bullets, citations, CSV, or a Markdown table.
9. Use the **menu-bar / tray icon** to show vbuff, copy the latest clip, clear history, pause/resume capture, toggle start-at-login, or quit.
10. **Press Esc** (or click away) to dismiss the popup without pasting.

The popup status line and the first disabled tray-menu row show whether capture is active, paused, starting, unavailable, or retrying a clipboard/history failure. The same compact popup line reports whether the detected security posture is protected, partial, or blocked. Command failures and copy-only fallback appear as dismissible notices; these messages never include clipboard payloads.

Run `vbuff doctor --json` for a content-free machine-readable startup, store/FTS, process-hardening, and security-capability report. Run `vbuff doctor` for the compact human-readable form; doctor does not start the resident UI or require the single-instance handoff.

Use `vbuff config export [file]` to emit a redacted, history-free TOML bundle; app exclusions and source matchers stay local. Apply a bounded bundle (up to 256 KiB) with `vbuff config apply <file>`, or pass `-` to read from stdin. Verify a downloaded artifact without starting the resident app:

```sh
vbuff verify --file ./vbuff --sha256 <64-hex-character-release-hash>
```

For an explicit second-machine setup transfer, `vbuff config handoff export setup.toml` writes the full configuration, including private matchers, with a checksum and owner-only permissions; transfer it through a trusted channel and run `vbuff config handoff apply setup.toml`. Unlike the redacted export, a handoff file is sensitive. Run `vbuff ask --json --limit 10 "meeting link"` for bounded local retrieval over clips whose capture policy explicitly permits AI processing; the current engine is local feature hashing, not a generative model.

The hotkey is rebindable in settings, with conflict detection at bind time. The popup opens near the cursor and is clamped to the work area. Pasting back is fully keyboard-driven: open, filter, navigate, paste, all without the mouse.

> **macOS paste-back permission.** Synthesizing Cmd+V into another app requires the **Accessibility** permission. On first run, grant vbuff access under System Settings -> Privacy & Security -> Accessibility. Until granted, vbuff runs in **copy-only** mode: selecting a clip puts it on the clipboard and you paste manually with Cmd+V. The onboarding flow deep-links you to the right settings pane.

---

## Configuration

Settings, hotkeys, exclusion lists and the per-app blacklist live in a human-editable **TOML config file** in your OS config directory (resolved via the platform's standard application directories). Configuration is policy and lives in the config file; clipboard history is data and lives in the SQLite database stored separately in your OS data directory:

| Platform | Config (TOML) | Data (history database) |
|---|---|---|
| macOS | `~/Library/Application Support/vbuff/` | `~/Library/Application Support/vbuff/vbuff.db` |
| Windows | `%APPDATA%\vbuff\` | `%APPDATA%\vbuff\vbuff.db` |
| Linux | `$XDG_CONFIG_HOME/vbuff/` (default `~/.config/vbuff/`) | `$XDG_DATA_HOME/vbuff/vbuff.db` (default `~/.local/share/vbuff/`) |

The target architecture adds an encrypted database, storage-location overrides, cloud-folder warnings, and stronger path validation before broader releases. The current config also exposes byte-aware capture limits, RSS soft/hard limits, structural-secret detection and TTL, and `strict_security_mode`; strict mode intentionally refuses capture while required protections such as encryption at rest remain unavailable.

Set `launch_at_login = true` in the config, or use the tray/menu-bar action, to register vbuff with the current OS login startup mechanism. The current MVP writes a LaunchAgent on macOS, a readiness-friendly XDG autostart desktop entry on Linux, or a user Run-key entry on Windows. A hardened `systemd --user` unit is also provided at `packaging/linux/vbuff.service` for package maintainers.

---

## Roadmap

Phased to ship a usable, private, single-machine clipboard manager first, then depth, then networked and team features. Each phase has explicit exit criteria; see [plan.md](plan.md) for the full milestone breakdown.

| Phase | Theme | Highlights |
|---|---|---|
| **Phase 0 - Foundations** | Scaffolding | Cargo workspace and crate skeleton, the four backend traits with mock backends, schema v1 + migrations, encrypted-store open path, content-hash golden vectors, core engine fully testable headless. |
| **MVP** | Single machine, the core loop everywhere | copy -> store -> hotkey -> popup -> paste-back, encrypted at rest, on macOS, Windows, X11 and Wayland (wlr-data-control). Capture all flavors, dedup, pin/favorite, substring search-as-you-type, plain/rich paste-back, tray, themes, accessibility tree, MVP snippets, MVP transforms, MVP privacy controls. |
| **v1** | A power user's daily driver | Files/custom MIME/source-app tagging, total-size + time retention, out-of-row blob CAS, FTS5 indexed search, fuzzy/regex search, tags/collections/pinboards, richer snippets and transforms, master password + idle auto-lock, i18n/RTL/a11y depth, scripting and integrations, and the first networked work: LAN P2P sync with encrypted transport and verified pairing. |
| **v2** | Across all my devices and my team | Flexible sync transports (relay / user cloud drive), conflict resolution, send-to-device, shareable links and QR handoff, shared team snippet libraries with roles and revocation, in-app updater, distribution polish. |

Sync features were tagged early in the raw feature list but depend on a stable single-machine core, so they are sequenced as the first networked work within v1 rather than in the MVP.

---

## Documentation

- [architecture.md](architecture.md) - full system design: process model, the four backend traits, data model, storage and search, security and threat model, crate dependency table, roadmap and risks.
- [plan.md](plan.md) - phased implementation plan and milestones.
- [recommendation.md](recommendation.md) - prioritized product and engineering recommendations.
- [docs/implementation-batch-001-050.md](docs/implementation-batch-001-050.md) - item-by-item implementation status, review corrections, and acceptance gate for the first batch.
- [docs/implementation-batch-051-100.md](docs/implementation-batch-051-100.md) - reliability, security, platform-capability, IPC, and plugin implementation status for the second batch.
- [docs/implementation-batch-101-150.md](docs/implementation-batch-101-150.md) - release verification, Trust UI, migration/sync contracts, policy decisions, and review evidence for the third batch.
- [docs/implementation-batch-151-200.md](docs/implementation-batch-151-200.md) - privacy/AI gates, integration contracts, delivery decisions, Compose workflows, and three review passes for the fourth batch.
- [docs/implementation-batch-201-250.md](docs/implementation-batch-201-250.md) - power-workflow contracts, responsive/accessibility UI, store lifecycle behavior, and three review passes for the fifth batch.
- [docs/implementation-batch-251-300.md](docs/implementation-batch-251-300.md) - everyday runtime UX, device/integration foundations, operations, and three review passes for the sixth batch.
- [docs/decision-gates-151-200.md](docs/decision-gates-151-200.md) - numeric stop/go rules, owner roles, fallback ladders, and external evidence boundaries.
- [docs/decision-gates-201-250.md](docs/decision-gates-201-250.md) - native caret, assistive-technology, plugin-host, display, and recovery-key gates.
- [docs/decision-gates-251-300.md](docs/decision-gates-251-300.md) - native auto-pause, live sync/client authority, release evidence, migration, and governance gates.
- [docs/limitations.md](docs/limitations.md) - versioned current-product limitations, practical workarounds, and exit evidence.
- [docs/maintainer-handoff.md](docs/maintainer-handoff.md) - release custody, emergency patch, dependency cadence, sunset, and handoff drill.
- [docs/scope-review.md](docs/scope-review.md) - quarterly Promote/Keep/Defer/Cut decisions and the mechanical breadth cut line.
- [docs/data-contract-v1.md](docs/data-contract-v1.md) - frozen schema/hash/format/IPC fixtures and compatibility procedure.
- [docs/data-contract-v2.md](docs/data-contract-v2.md) - schema 6 normalized-dedup, encrypted grace-bin, retention, and migration contract.
- [docs/product-strategy-decisions.md](docs/product-strategy-decisions.md) - coherent licensing, pricing, and governance decisions for mutually exclusive items 128-140.
- [docs/competitive-analysis.md](docs/competitive-analysis.md) - competitor landscape and the four-corner gap.
- [docs/competitor-extras.md](docs/competitor-extras.md) - 122 additional/advanced competitor features and their suggested priority.
- [docs/features-top-500.md](docs/features-top-500.md) - the 640-feature catalog with priority tiers.
- [docs/ideas-top-300.md](docs/ideas-top-300.md) - ideas 198-300 in the extended backlog.
- [docs/ideas-301-400.md](docs/ideas-301-400.md) - ideas 301-400 in the extended backlog.
- [docs/ideas-401-500.md](docs/ideas-401-500.md) - review backlog items 401-500: problems, SOLID/DRY cuts, UX/design fixes, and roadmap hygiene.
- [docs/ideas-501-600.md](docs/ideas-501-600.md) - evidence-backed backlog items 501-600: native correctness, international text/search, privacy, sync, and release verification.
- [docs/ideas-601-610.md](docs/ideas-601-610.md) - ten evidence-backed post-600 candidates kept outside the active 1-600 goal.
- [docs/ideas-611-620.md](docs/ideas-611-620.md) - ten review-derived state-machine, replay, configuration, and release-evidence candidates, also outside the active goal.
- [docs/repositories-research-100.md](docs/repositories-research-100.md) - 100 verified high-signal repositories plus the scientific papers, standards, and concrete lessons behind items 501-600.
- [docs/mistakes-top-500.md](docs/mistakes-top-500.md) - competitor anti-patterns and the vbuff decision that prevents each.

---

## Contributing

Contributions are welcome. vbuff is in early development, so the highest-leverage way to help is to pick up work from the current milestone in [plan.md](plan.md), or to file an issue describing a bug, a platform quirk (especially on specific Linux compositors), or a feature from the catalog you want prioritized.

A few ground rules grounded in the project's design:
- `vbuff-core` must stay free of OS-specific and GUI code; platform behavior goes behind the backend traits in `vbuff-platform`.
- Capture-path changes must preserve the fail-closed privacy guarantees and byte-for-byte fidelity; the canary at-rest encryption test and the fail-closed capture-gate tests must stay green.
- Run `cargo fmt`, `cargo clippy`, and `cargo test --workspace` before opening a pull request.

A more detailed `CONTRIBUTING.md` will follow as the project stabilizes.

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
