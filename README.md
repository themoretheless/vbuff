# vbuff

**One clipboard, every machine. Never lost, never leaked.**

A fast, private, cross-platform clipboard manager written in Rust. vbuff captures every clipboard change into a durable, searchable, encrypted local history, summons a keyboard-driven popup with a global hotkey, and pastes the chosen clip straight back into the app you were just using. Local-first and private by default, with opt-in peer-to-peer sync planned for later phases.

---

## What is vbuff, and why it exists

Clipboard managers are a mature but deeply fragmented category, and no single product covers the things that matter at once. The best tool on each platform is usually *only* on that platform: **Ditto** is the de facto free manager on Windows but is Windows-only and local-only; **Paste** is beautifully polished and syncs across Apple devices but is macOS/iOS-only and subscription-only; **CopyQ** genuinely spans macOS, Windows and Linux but wears a dated, scripting-heavy UI and has no real sync; **CrossPaste** reaches every platform and syncs privately but only over the LAN and without the polish. A person who works across macOS, Windows and Linux cannot carry one mental model, one keybinding scheme, or one private history across all three.

That is the four-corner gap vbuff is built to close: **be truly cross-platform (macOS + Windows + Linux), genuinely polished, privately synced, and approachable, all at the same time.** Every competitor wins at most two of those corners; vbuff aims to occupy all four. The non-negotiable foundation is privacy: a clipboard manager sees every password, OTP, API key and private message that transits the clipboard, so vbuff defaults to local-only, encrypts at rest, honors OS "do not store" hints before a single byte touches disk, and treats "do not capture" as a first-class, fail-closed code path. Cross-device sync, when it arrives, is opt-in and end-to-end encrypted, never a vendor backend that can read your clips or be shut down.

---

## Platform support

vbuff is one codebase with native, per-OS backends behind common Rust traits. The popup, search and storage are identical everywhere; only the clipboard, hotkey, paste-back and tray plumbing differ per platform.

| Platform | Clipboard capture | Global hotkey | Paste-back | Notes |
|---|---|---|---|---|
| **macOS** | `NSPasteboard` `changeCount` polling (~150-250 ms, adaptive idle backoff) | Carbon `RegisterEventHotKey` | Focus restore + synthetic Cmd+V | Paste-back needs **Accessibility** permission (granted in System Settings). Honors `org.nspasteboard.ConcealedType` / `TransientType`. |
| **Windows** | `AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE` (event-driven) | Win32 `RegisterHotKey` | `SetForegroundWindow` + `SendInput` Ctrl+V | Honors `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory`. |
| **Linux / X11** | `CLIPBOARD` selection ownership via XFIXES selection events | `XGrabKey` | `XTEST` Ctrl+V | vbuff takes selection ownership so clips survive the source app closing. |
| **Linux / Wayland** | `wlr-data-control` (wlroots: Sway, Hyprland, river; also KDE Plasma) | `GlobalShortcuts` portal (`xdg-desktop-portal`) | virtual-keyboard / `wtype` / `ydotool`, else set-and-let-user-paste | See the GNOME caveat below. |

> **GNOME on Wayland caveat.** GNOME's Mutter compositor does not implement `wlr-data-control` and offers no sanctioned background clipboard-monitor API. On GNOME-Wayland, vbuff degrades gracefully to **capture-on-summon** (it reads the clipboard when the popup opens, since the popup has focus) plus a manual capture hotkey, and it shows an honest in-app explanation of what is and is not being captured rather than silently dropping copies. This is a genuine platform limitation, not a bug. X11 sessions and wlroots/KDE Wayland sessions are unaffected.

Source-app attribution and per-app exclusion rely on knowing the foreground app, which Wayland intentionally hides from clients. On Wayland those features are best-effort and clearly badged as unavailable where the compositor cannot provide the information; content-pattern and secret-detection rules still apply.

---

## Feature highlights

Curated from the project's feature catalog (the strongest MVP and v1 items, not the full 640). Items beyond the MVP are marked with their target phase.

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

- **Encrypted at rest** with the key held in the OS secret store, not beside the database.
- **Honors OS concealed/secure markers** and password-field hints, skips designated apps, supports regex/keyword exclusion rules and built-in secret detection.
- **Local by default, zero telemetry, no network calls** out of the box.
- Auto-clear-on-timer, wipe-on-demand, and shorter retention for sensitive clips.
- Cross-device, end-to-end encrypted sync is planned and opt-in (v1 foundation, v2 breadth), never the default path and never a backend that can read your data.

---

## Target privacy and security

vbuff is designed around a single hard rule: **fail closed.** Every uncertainty in "should we capture this?" should resolve to *do not capture*, and the decision must run before any byte touches durable storage. The current repository has the first pieces of that model (pause, app exclusion, whitespace skipping, dedup, local SQLite history); the full target adds OS concealed/transient hints, a default secret-tool deny-list, regex/keyword rules, built-in secret detectors, encrypted-at-rest storage with the key in the OS secret store, secure delete, and opt-in end-to-end encrypted sync. vbuff defends against stolen disks, other unprivileged local users and on-the-wire interception once those target controls land; it does not claim to defend against a root/admin attacker, a debugger attached to its own process, or a kernel-level attacker on the same machine.

---

## Status

vbuff is in active early development. The repository already contains a Cargo workspace with `vbuff-types`, `vbuff-core`, `vbuff-store`, `vbuff-platform`, `vbuff-gui`, and a root single-process binary. The current executable polls the clipboard through `arboard`, captures text/images, stores history in a compact `rusqlite` schema, opens an `egui` popup through a global hotkey, and writes the selected clip back before invoking an `enigo` paste keystroke. It also owns a minimal single-instance endpoint: a duplicate launch forwards `ShowPopup` to the resident process and exits, stale endpoints are recovered once, and a capture heartbeat makes a stalled worker visible. Native all-flavor clipboard backends, SQLCipher encryption, full per-OS parity, the formal daemon/IPC split, CLI, and sync remain target work tracked in [architecture.md](architecture.md) and [plan.md](plan.md).

---

## Architecture at a glance

vbuff is a Cargo **workspace** with a fat, OS-agnostic core and thin platform crates. The cardinal rule: `vbuff-core` contains zero OS-specific code and zero GUI code, so the bulk of the logic is unit- and property-testable on any host with mock backends.

| Crate | Role | In MVP? |
|---|---|---|
| `vbuff-types` | Plain shared clip, status, notice, and minimal IPC contracts; serde only | Yes |
| `vbuff-core` | Current pure logic: dedup, eviction, classification, substring search; target adds redaction rules, transforms, snippet expansion | Yes (partial) |
| `vbuff-store` | Current bundled SQLite JSON-flavor store; target adds SQLCipher, FTS5, migrations, blob spill, at-rest crypto | Yes (partial) |
| `vbuff-platform` | Current trait layer plus `arboard` / `global-hotkey` / `enigo` adapters; target adds native per-OS clipboard, hotkey, paste and tray impls | Yes (partial) |
| `vbuff-gui` | Current `eframe` popup; target adds deeper settings viewports, richer badges, accessibility depth | Yes (partial) |
| *(root app)* | `src/main.rs` composes startup; focused modules own capture supervision, history, commands, paste timing, event-loop wiring, autostart, tray/menu-bar integration, and minimal single-instance handoff | Yes |
| `vbuff-daemon` | Background wiring, IPC server, single-instance guard (as the model splits out) | Later |
| `vbuff-ipc` | Full framed control protocol over Unix socket / Windows named pipe; the root app currently carries only `ShowPopup`/`Ping` bootstrap framing | Later |
| `vbuff-sync` | mDNS discovery, Noise/TLS transport, pairing, LAN P2P replication | Later |
| `vbuff-cli` | `vbuff` verbs as a pure IPC client | Later |

The GUI is **egui** rendered via **eframe**. Immediate mode is a natural fit for a search-as-you-type list: each keystroke re-filters the rows with no retained widget tree to diff, and `ScrollArea::show_rows` gives row virtualization for free. The current MVP store is **SQLite** via bundled `rusqlite`; target work adds SQLCipher, FTS5, and an out-of-row content-addressable blob store. Dedup already uses **BLAKE3**. See [architecture.md](architecture.md) for the full design, data model and crate dependency table.

---

## Read the project in small pieces

The repo is intentionally split so you can understand it without loading the whole product into your head at once:

1. **Data shapes and wire contracts:** start with `crates/vbuff-types/src/lib.rs`, then `status.rs` and `ipc.rs` (`Clip`, flavors, ids, `CaptureHealth`, redacted notices, and the minimal startup intents).
2. **Pure behavior:** read `crates/vbuff-core/src/*` for hashing, classification, filtering, and eviction. This crate should stay OS-free and GUI-free.
3. **Persistence:** read `crates/vbuff-store/src/lib.rs` for the current SQLite MVP store and its transitional JSON-flavor schema.
4. **Platform ports:** read `crates/vbuff-platform/src/traits.rs` first; native per-OS backends should hang behind those traits.
5. **GUI state and rendering:** read `crates/vbuff-gui/src/state.rs`, then `design.rs`, `view.rs`, and `app.rs`.
6. **History boundary:** read `src/history.rs`; it is the only app-layer facade that couples store mutations to refreshed GUI snapshots.
7. **Resident workflows:** read `src/capture.rs` for polling, policy, heartbeat/watchdog supervision, and `src/paste.rs` for clipboard-write-before-delayed-paste sequencing.
8. **Diagnostics publisher:** read `src/diagnostics.rs`; capture and command handling publish typed status through this narrow boundary instead of depending on GUI internals.
9. **Startup handoff:** read `src/single_instance/mod.rs` for framing/ownership, then `unix.rs` or `windows_fallback.rs` for one transport; this slice owns bind-or-forward, liveness, stale recovery, and cleanup.
10. **Shared commands and OS surfaces:** read `src/commands.rs`, then `src/tray.rs` and `src/autostart.rs`.
11. **Composition shell:** read `src/app.rs`, then `src/main.rs` last. `main.rs` loads dependencies and starts the focused modules; it does not contain workflow logic.

The SOLID/DRY rule of thumb is simple: data and serializable status/IPC contracts live in `vbuff-types`, pure logic is testable, platform code is behind traits, storage owns SQL, GUI owns presentation, `AppCommand` is the one command vocabulary, single-instance transport stays isolated, and `main.rs` only composes the pieces.

## Design direction

vbuff should feel like a quiet resident tool, not a marketing page or a scripting console. The first screen is the usable popup: dense enough for repeated work, calm enough for secrets, and fully keyboard-driven.

The current design baseline is implemented rather than aspirational: one token module controls popup dimensions, row height, spacing, thumbnail size, and icon-button size; rows do not resize when pin/delete controls appear; emoji actions were replaced with font-independent native icons, accessibility labels, and tooltips; empty/search-empty states are distinct; and delete/clear actions require explicit confirmation (the latter says pinned clips are preserved). The popup and tray show the same typed capture-health state (`Capture active`, starting, stalled, unavailable, read issue, or history-write issue), while redacted command notices report copy/paste/delete/clear/autostart outcomes without exposing clip content. The tray/menu-bar uses a recognizable clipboard/check glyph, the same command wording as the popup, and routes destructive clearing through the popup confirmation.

- **Popup:** stable row dimensions, clear selected state, type icon, source/time metadata, and small state badges for pinned, sensitive, paused, degraded, local-only, and synced.
- **Actions:** repeated row tools should use icon buttons with tooltips; destructive actions such as clear history, wipe, revoke, or reset should use explicit text and confirmation.
- **Density:** compact and comfortable modes should share one layout system instead of ad hoc spacing.
- **Accessibility:** focus rings, high contrast, screen-reader labels, reduced motion, and pointer-free navigation are part of the core design, not a later cleanup.
- **Trust:** privacy state should be visible at decision points. A sensitive row should look protected and understandable, not alarming or hidden behind mystery UI.

---

## 500-point review backlog

The 500 proposals, improvements, problems, bugs and "done badly" notes are kept as one numbered backlog, split by decision level so each file stays readable. Treat this as review input, not an automatic scope increase; `plan.md` decides what graduates into implementation.

| Range | Canonical file | Lens |
|---|---|---|
| 1-113 | [architecture.md](architecture.md) | Engineering architecture, backends, storage, privacy, sync, testability |
| 114-197 | [recommendation.md](recommendation.md) | Product strategy, differentiation, business model, roadmap tradeoffs |
| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) | User workflows, UI/UX, sync experience, integrations, operations |
| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) | Privacy controls, search, data model, platform fit, teams, automation |
| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) | Current implementation problems, SOLID/DRY slices, design fixes, review hygiene |

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
8. Use the **menu-bar / tray icon** to show vbuff, copy the latest clip, clear history, pause/resume capture, toggle start-at-login, or quit.
9. **Press Esc** (or click away) to dismiss the popup without pasting.

The popup status line and the first disabled tray-menu row show whether capture is active, paused, starting, unavailable, or retrying a clipboard/history failure. Command failures and copy-only fallback appear as dismissible notices; these messages never include clipboard payloads.

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

The target architecture adds an encrypted database, storage-location overrides, cloud-folder warnings, and stronger path validation before broader releases.

Set `launch_at_login = true` in the config, or use the tray/menu-bar action, to register vbuff with the current OS login startup mechanism. The current MVP writes a LaunchAgent on macOS, an XDG autostart desktop entry on Linux, or a user Run-key entry on Windows.

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
- [docs/competitive-analysis.md](docs/competitive-analysis.md) - competitor landscape and the four-corner gap.
- [docs/competitor-extras.md](docs/competitor-extras.md) - 122 additional/advanced competitor features and their suggested priority.
- [docs/features-top-500.md](docs/features-top-500.md) - the 640-feature catalog with priority tiers.
- [docs/ideas-top-300.md](docs/ideas-top-300.md) - ideas 198-300 in the extended backlog.
- [docs/ideas-301-400.md](docs/ideas-301-400.md) - ideas 301-400 in the extended backlog.
- [docs/ideas-401-500.md](docs/ideas-401-500.md) - review backlog items 401-500: problems, SOLID/DRY cuts, UX/design fixes, and roadmap hygiene.
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
