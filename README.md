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

> **Target design, not current state.** The table below is the per-OS native backend architecture vbuff is being
> built toward. The current repository ships exactly one cross-platform clipboard backend (`arboard`, polling,
> text + single-image only, no concealed-hint support) and one hotkey backend (`global-hotkey`, which does not
> cover Wayland). None of the native XFIXES/`wlr-data-control`/`AddClipboardFormatListener` paths, and none of the
> concealed-hint honoring, exist yet. See [docs/code-audit-top-50.md](docs/code-audit-top-50.md) items #11-#20.

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

> **Reading this section.** These are the *target* MVP/v1 feature set, not all shipped yet. Bullets marked
> **(target)** describe design intent with no corresponding code in the repo today; see
> [docs/code-audit-top-50.md](docs/code-audit-top-50.md) for the full, file-and-line-grounded gap list between this
> page and the current binary.

### Capture everything, byte-for-byte

- Background watcher captures clipboard changes via a fixed-interval `arboard` poll today (target: event-driven per-OS backends, near-0%-idle CPU - **target**, see audit #11-13).
- Currently captures **plain text and a single raster image** per copy via `arboard`; rich text/HTML, RTF, files/folders, custom MIME types and color clips are **target** (audit #14-15).
- Atomic multi-flavor capture (HTML + plain text + image together) is **target**; today only one flavor is ever stored per copy (audit #14).
- **Byte-for-byte fidelity**: no whitespace, newline or encoding normalization, so editor selections and exact payloads round-trip.
- **Deduplicates** re-copied content (move-to-top instead of a duplicate row) via a BLAKE3 content hash.
- **Pause / resume** works today. Incognito mode and a manual capture-on-demand hotkey are **target** (audit #7-8).

### Instant keyboard-driven recall

- Global hotkey opens a popup near the cursor that **filters as you type**, with live match highlighting.
- Fully keyboard-driven: navigate, filter, paste-back and pin without touching the mouse.
- **Number-key quick-pick** for the top items, and per-action shortcuts (paste, paste-plain, pin, delete).
- Type / pinned / favorite filters, empty and no-results states, dark/light themes, image thumbnails, per-type icons.
- Search today is a client-side substring filter over the most recent 1,000 clips; FTS5 indexed search for 100k+ items is **target**, and no FTS5 schema exists yet (audit #21-23).

### Reliable paste-back

- Restores focus to the previously active app and injects a real paste, so the clip lands where you were typing.
- **Plain vs keep-formatting** paste, with a one-shot paste-as-plain action.
- **Enter** to paste-back, double-click to paste, or a number key for quick-pick; **copy-only** fallback when paste injection is unavailable.
- Self-write suppression is **target**; today a paste triggers a harmless but needless re-insert cycle on the next capture-thread poll (audit #44).

### Organization that survives restarts

- **Pin to top** and **star as favorite**; pinned items are exempt from eviction and persist across restarts as a reusable snippet bank.
- Promote a clip to a **permanent** item that never auto-prunes.
- Tags, folders/collections, named tabs, pinboards, notes, color labels and manual drag-reorder arrive in v1.
- Configurable retention: count cap, total-size cap, time expiry, or unlimited mode (pins/favorites always exempt). Today only a count cap is implemented; total-size cap and time expiry are **target**.

### Snippets and quick transforms (growing through v1)

- Saved snippets with abbreviation expansion, insert-by-hotkey, folders and a built-in editor; date/time placeholders in the MVP set. **(target - not in this repo yet)**
- Promote any clip into a snippet in one keystroke. **(target)**
- Quick-action palette with change-case, trim whitespace, strip formatting and literal find-and-replace; programmer-case, regex replace, base64/URL encode-decode and JSON pretty-print expand the set in v1. **(target - no transform code exists yet)**
- One product instead of a separate clipboard manager *and* a separate text expander.

### Private and trustworthy by construction

- **Encrypted at rest** is a **target**, not shipped: the current store is a plain, unencrypted SQLite database with no key or secret-store integration of any kind (audit #1-2).
- **Honors OS concealed/secure markers**: **target**, not implemented (audit #3). Per-app exclusion rules exist in config but never actually trigger today because the source app is never captured (audit #4); there is no default deny-list (audit #5), and regex/keyword rules and secret detection do not exist yet (audit #6).
- **Local by default, zero telemetry, no network calls** out of the box - true today; there is no networking code in the repo at all.
- Auto-clear-on-timer, wipe-on-demand, and shorter retention for sensitive clips are **target** (audit #9).
- Cross-device, end-to-end encrypted sync is planned and opt-in (v1 foundation, v2 breadth), never the default path and never a backend that can read your data.

---

## Target privacy and security

vbuff is designed around a single hard rule: **fail closed.** Every uncertainty in "should we capture this?" should resolve to *do not capture*, and the decision must run before any byte touches durable storage. The current repository has the first pieces of that model (pause, whitespace skipping, dedup, local SQLite history) - **not yet app exclusion**, which is implemented and unit-tested but never actually triggers today because the capture backend never reports which app a copy came from (see [docs/code-audit-top-50.md](docs/code-audit-top-50.md) #4). The full target adds OS concealed/transient hints, a working default secret-tool deny-list, regex/keyword rules, built-in secret detectors, encrypted-at-rest storage with the key in the OS secret store, secure delete, and opt-in end-to-end encrypted sync - none of which exist in the repository yet. Until those target controls land, vbuff does not defend against anything beyond accidental disclosure to another process reading the same plaintext SQLite file; it does not claim to defend against a root/admin attacker, a debugger attached to its own process, or a kernel-level attacker on the same machine.

---

## Status

vbuff is in active early development. The repository already contains a Cargo workspace with `vbuff-types`, `vbuff-core`, `vbuff-store`, `vbuff-platform`, `vbuff-gui`, and a root single-process binary. The current executable polls the clipboard through `arboard` on a fixed timer, captures text or a single image, stores history in a compact unencrypted `rusqlite` schema (no SQLCipher, no FTS5), opens an `egui` popup through a global hotkey, and writes the selected clip back before invoking an `enigo` paste keystroke. There is no CI configuration and no mock platform backends yet, so the OS-facing code paths (clipboard, hotkey, paste) are untested. Native all-flavor clipboard backends, SQLCipher encryption, full per-OS parity, the formal daemon/IPC split, CLI, and sync remain target work tracked in [architecture.md](architecture.md) and [plan.md](plan.md); see [docs/code-audit-top-50.md](docs/code-audit-top-50.md) for the full, evidence-grounded list of what's missing or diverges from the docs today.

---

## Architecture at a glance

vbuff is a Cargo **workspace** with a fat, OS-agnostic core and thin platform crates. The cardinal rule: `vbuff-core` contains zero OS-specific code and zero GUI code, so the bulk of the logic is unit-testable on any host without touching the OS. Mock implementations of the four `vbuff-platform` traits are a **target** (not present in the repo yet - [docs/code-audit-top-50.md](docs/code-audit-top-50.md) #34), so today only pure logic is tested; the real clipboard/hotkey/paste code paths have no automated test coverage.

| Crate | Role | In MVP? |
|---|---|---|
| `vbuff-types` | Plain shared data types (`Clip`, `Flavor`, `ContentKind`, ids); serde only | Yes |
| `vbuff-core` | Engine: dedup, eviction, retention, search, redaction rules, transforms, snippet expansion (pure logic + trait calls) | Yes |
| `vbuff-store` | SQLite + SQLCipher persistence, FTS5, migrations, blob spill, at-rest crypto | Yes |
| `vbuff-platform` | The four backend trait definitions + per-OS impls (clipboard, hotkey, paste, tray) | Yes |
| `vbuff-gui` | `eframe` app: popup + settings viewports | Yes |
| *(root binary)* | `src/`: thin startup wiring (`main.rs`) plus one file per concern - `capture.rs`, `actions.rs`, `gui.rs`, `tray.rs`, `config.rs` | Yes |
| `vbuff-daemon` | Background wiring, IPC server, single-instance guard (as the model splits out) | Later |
| `vbuff-ipc` | Framed protocol over Unix socket / named pipe | Later |
| `vbuff-sync` | mDNS discovery, Noise/TLS transport, pairing, LAN P2P replication | Later |
| `vbuff-cli` | `vbuff` verbs as a pure IPC client | Later |

The GUI is **egui** rendered via **eframe**. Immediate mode is a natural fit for a search-as-you-type list: each keystroke re-filters the rows with no retained widget tree to diff, and `ScrollArea::show_rows` gives row virtualization for free. Storage is **SQLite** via `rusqlite` (bundled; SQLCipher and FTS5 are **target**, not yet wired in - see [docs/code-audit-top-50.md](docs/code-audit-top-50.md) #1 and #21); dedup uses **BLAKE3**; large payloads spilling to an out-of-row, content-addressable blob store is also **target** (the `Body::Spilled` variant exists but nothing constructs it yet). See [architecture.md](architecture.md) for the full design, data model and crate dependency table, and its "Current module map" section for a file-by-file breakdown of every crate - each source file was split to hold one Single-Responsibility-Principle-sized job, so the whole codebase can be read one small file at a time instead of a handful of 400+ line files.

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
8. Use the **menu-bar / tray icon** to show vbuff, copy the latest clip, clear history, pause/resume capture, or quit.
9. **Press Esc** (or click away) to dismiss the popup without pasting.

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
- [docs/ideas-301-400.md](docs/ideas-301-400.md) - ideas 301-400 extending the backlog to 400.
- [docs/mistakes-top-500.md](docs/mistakes-top-500.md) - competitor anti-patterns and the vbuff decision that prevents each.
- [docs/code-audit-top-50.md](docs/code-audit-top-50.md) - top 50 things wrong in *this repo's own code* today, each grounded in a file and line, cross-referenced against the claims made in this README, `architecture.md`, and `recommendation.md`.
- [docs/problems-improvements-top-500.md](docs/problems-improvements-top-500.md) - 506 more, extending the 50 above (items 51-556 combined): SOLID/DRY architecture, security, platform parity, storage, concurrency, performance, testing, Rust idiom, config UX, GUI/visual design, docs, and dependency/supply-chain findings, generated by a 16-lens multi-agent pass and cross-checked (`cargo audit` independently confirmed every advisory it cites).

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
