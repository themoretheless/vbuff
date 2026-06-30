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

## Privacy and security

vbuff is built around a single hard rule: **fail closed.** Every uncertainty in "should we capture this?" resolves to *do not capture*, and the decision runs before any byte is hashed or read into long-lived memory. The most damaging failure a clipboard manager can have is a silent leak, so vbuff layers its defenses: it honors OS concealed/transient hints from well-behaved password managers (`org.nspasteboard.ConcealedType` on macOS, `ExcludeClipboardContentFromMonitorProcessing` on Windows, password-manager hint targets on Linux), ships a sane default exclude list for known secret tools, supports per-app and regex/keyword exclusion, runs built-in secret detectors, and offers incognito and pause toggles. History is stored in an owner-only, per-user data location (file modes 0600); the database is encrypted at rest with the key kept in the OS keychain rather than next to the ciphertext; deleting a clip scrubs the underlying storage rather than leaving recoverable residue. The local database is never routed through a cloud-sync folder (that path corrupts SQLite); when sync ships it operates at the record level with end-to-end encryption and pairing verification, so a relay only ever sees ciphertext. vbuff defends against stolen disks, other unprivileged local users and on-the-wire interception; it does not claim to defend against a root/admin attacker, a debugger attached to its own process, or a kernel-level attacker on the same machine.

---

## Status

vbuff is in active early development. The architecture defines a full multi-crate Cargo workspace, but the **shippable MVP is a single-process subset**: the long-lived clipboard daemon, GUI popup and settings all run inside one binary that watches the clipboard, writes to an encrypted SQLite store, owns the global hotkey, and pastes back, on macOS, Windows, Linux/X11 and Linux/Wayland (with the GNOME caveat above). The separate IPC, scriptable CLI and peer-to-peer sync crates are later phases. Expect rough edges, changing schemas (migrations are forward-only and back themselves up), and platform features that degrade visibly rather than pretend to work.

---

## Architecture at a glance

vbuff is a Cargo **workspace** with a fat, OS-agnostic core and thin platform crates. The cardinal rule: `vbuff-core` contains zero OS-specific code and zero GUI code, so the bulk of the logic is unit- and property-testable on any host with mock backends.

| Crate | Role | In MVP? |
|---|---|---|
| `vbuff-types` | Plain shared data types (`Clip`, `Flavor`, `ContentKind`, ids); serde only | Yes |
| `vbuff-core` | Engine: dedup, eviction, retention, search, redaction rules, transforms, snippet expansion (pure logic + trait calls) | Yes |
| `vbuff-store` | SQLite + SQLCipher persistence, FTS5, migrations, blob spill, at-rest crypto | Yes |
| `vbuff-platform` | The four backend trait definitions + per-OS impls (clipboard, hotkey, paste, tray) | Yes |
| `vbuff-gui` | `eframe` app: popup + settings viewports | Yes |
| *(root binary)* | `src/main.rs`: launches the single-process app, owns the watcher/store/GUI | Yes |
| `vbuff-daemon` | Background wiring, IPC server, single-instance guard (as the model splits out) | Later |
| `vbuff-ipc` | Framed protocol over Unix socket / named pipe | Later |
| `vbuff-sync` | mDNS discovery, Noise/TLS transport, pairing, LAN P2P replication | Later |
| `vbuff-cli` | `vbuff` verbs as a pure IPC client | Later |

The GUI is **egui** rendered via **eframe**. Immediate mode is a natural fit for a search-as-you-type list: each keystroke re-filters the rows with no retained widget tree to diff, and `ScrollArea::show_rows` gives row virtualization for free. Storage is **SQLite** via `rusqlite` (bundled, with SQLCipher and FTS5); dedup uses **BLAKE3**; large payloads spill to an out-of-row, content-addressable blob store. See [architecture.md](architecture.md) for the full design, data model and crate dependency table.

---

## Build from source

vbuff is a standard Cargo workspace. You need a recent stable **Rust toolchain** (install via [rustup](https://rustup.rs)) plus a few per-OS native dependencies.

### Prerequisites

**macOS**
- Xcode Command Line Tools: `xcode-select --install`
- A recent stable Rust toolchain. No extra packages are required to build; the encrypted store vendors SQLite/SQLCipher.

**Windows**
- Rust with the MSVC toolchain (the default from rustup) and the **Visual Studio Build Tools** (the "Desktop development with C++" workload) for the C/C++ linker.
- No additional system libraries are required for the bundled store.

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
8. **Press Esc** (or click away) to dismiss the popup without pasting.

The hotkey is rebindable in settings, with conflict detection at bind time. The popup opens near the cursor and is clamped to the work area. Pasting back is fully keyboard-driven: open, filter, navigate, paste, all without the mouse.

> **macOS paste-back permission.** Synthesizing Cmd+V into another app requires the **Accessibility** permission. On first run, grant vbuff access under System Settings -> Privacy & Security -> Accessibility. Until granted, vbuff runs in **copy-only** mode: selecting a clip puts it on the clipboard and you paste manually with Cmd+V. The onboarding flow deep-links you to the right settings pane.

---

## Configuration

Settings, hotkeys, exclusion lists, regex rules and the per-app blacklist live in a human-editable **TOML config file** in your OS config directory (resolved via the platform's standard application directories). Configuration is policy and lives in the config file; the clipboard history is data and lives in the encrypted database, which is stored separately in your OS data directory:

| Platform | Config (TOML) | Data (history database) |
|---|---|---|
| macOS | `~/Library/Application Support/vbuff/` | `~/Library/Application Support/vbuff/vbuff.db` |
| Windows | `%APPDATA%\vbuff\` | `%APPDATA%\vbuff\vbuff.db` |
| Linux | `$XDG_CONFIG_HOME/vbuff/` (default `~/.config/vbuff/`) | `$XDG_DATA_HOME/vbuff/vbuff.db` (default `~/.local/share/vbuff/`) |

You can override the storage location; vbuff validates that the target is writable and supports atomic renames, and warns if you point it at a known cloud-sync folder (which corrupts the live database).

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


---

## Ideas and future directions

> Items 198-246 of a 246-idea backlog of suggested features and improvements. The full backlog: engineering ideas (1-113) in [architecture.md](architecture.md), product/strategy ideas (114-197) in [recommendation.md](recommendation.md). Effort tags: `S`/`M`/`L`.

### Power-user & workflow features

198. **Live paste-stack scratchpad window** `[M]` - A small always-visible dockable panel showing the current paste stack/queue as editable rows you can reorder, edit inline, delete, or duplicate before you start popping items, instead of the catalogued blind pop-and-paste. _Value: Catalogued paste stacks are invisible until you paste; surfacing the queue as a directly manipulable buffer lets users assemble a multi-field paste (form-filling, mail-merge by hand) and fix mistakes before committing - reduces the 'I popped the wrong order' failure that plagues Paste/Comfort stack UIs._
199. **Form-fill capture mode (round-trip a whole form)** `[L]` - A 'record fields' toggle that captures every copy made during a session as an ordered named slot, then a single 'replay into next form' walks Tab+paste through a target form using the captured order. _Value: Power users re-entering the same data across web forms, installers, and CRMs get a deterministic Tab-paste-Tab macro without writing a Keyboard Maestro macro or a DSL - directly serves the 'power without a second app' bet._
200. **Merge clips with structured templates, not just separators** `[M]` - Beyond the catalogued separator-merge, let users pick a merge template like '- {clip}\n', '[{n}] {clip}', or a 2-column table, so selected clips render as a Markdown list, numbered citations, or a CSV row in one action. _Value: Turns ad-hoc 'I copied 6 things and need a bulleted list / a table / numbered footnotes' into one click; the separator-only merge already in the catalog can't produce indices, wrappers, or columnar output._
201. **Two-clip diff and side-by-side compare** `[M]` - Multi-select exactly two text clips and open an inline diff view (line/word level) with a 'copy the merged/left/right result' action. _Value: Developers and writers constantly copy two versions of a config, JSON, or paragraph and want to see what changed; doing it inside the clipboard manager avoids a round-trip to a diff tool and is a transform no competitor offers from history._
202. **Transform preview as a non-destructive overlay on the row** `[M]` - When a transform/quick-action is highlighted, show its result as a ghost overlay on the actual history row (with a keyboard toggle to cycle candidate transforms) so the before/after is seen in place before Enter pastes it. _Value: The catalog has a generic 'preview before paste'; doing it as an in-list ghost that respects the 'never mutate canonical bytes' rule lets users audition case/regex/JSON transforms at a glance, raising the polish bar over CopyQ's modal previews._
203. **Conditional snippet fields (show field B only if A=yes)** `[M]` - Extend the catalogued fill-in forms with simple visibility rules - a checkbox or dropdown field can reveal, hide, or pre-fill other fields - configured entirely in the GUI form builder, no scripting. _Value: Real templates (support replies, contracts, on-call handoffs) have optional sections; TextExpander needs nesting tricks for this. A GUI rule keeps the 'no DSL' promise while covering the most common branching case._
204. **Computed/derived snippet fields** `[M]` - A snippet field can be marked computed: its value is another field transformed (uppercase, slugify, today+N days, char count) using the same transform palette already shipped for clips, evaluated live as the user types into the form. _Value: Eliminates copy-paste-retype inside a form (e.g. type a title once, get a slug, an UPPER header, and a filename); reuses existing transform code so it is incremental, and beats expanders that only do date math._
205. **Scripted actions as a visual node chain (GUI pipeline builder)** `[L]` - A drag-to-connect node editor where each node is an existing one-click transform, and the saved chain becomes a named quick-action - the catalogued 'transform pipeline' but authored visually instead of as a serialized list. _Value: Directly executes Bet 4 ('power without a DSL'): users compose strip-tracking-params -> shorten -> QR without learning CopyQ's JS; the visual graph is also self-documenting and shareable as a small file._
206. **Per-action sandbox & permission prompt for shell/script actions** `[L]` - Any user action that shells out runs under an explicit, per-action permission grant (network: no, filesystem: read-only, timeout, no env inheritance) shown in a one-line capability badge on the action. _Value: vbuff's whole pitch is private-by-default; the catalogued 'run shell command on clip' is a footgun without guardrails. A capability badge makes scripted actions safe to share and defensible to a security team that banned Ditto._
207. **Dry-run / explain for any quick action** `[S]` - Hold a modifier when invoking an action to get a plain-language 'this will: lowercase 412 chars, replace 3 matches, run no network' summary plus a diff, with nothing pasted. _Value: Builds the trust that the recommendation says new entrants must signal; users can verify a regex or a shared scripted action does what they think before it touches a live document._
208. **Multi-select rubber-band and range selection with running preview** `[S]` - Click-drag a marquee or Shift-range over history rows; a status strip shows live aggregate (N items, total chars, combined size) and the merged result preview updates as the selection changes. _Value: Catalogued multi-select only enables bulk organize; adding aggregate feedback and a live merge preview turns selection into a composition tool and answers 'how big is this batch' before a bulk paste or export._
209. **Pin board as a fillable layout grid** `[M]` - A pin board mode where pins are arranged in a fixed grid mapped to number-row hotkeys (1-9 across, rows via modifier), so a board becomes a muscle-memory keypad of canned responses, signatures, and commands. _Value: Beyond the catalogued list-style pinboards, spatial+hotkey layout gives the speed of physical Stream-Deck-style recall that PasteBar users want, while staying keyboard-only and cross-platform._
210. **Context-aware boards that auto-activate per app/window** `[M]` - A pin board can be tagged with target apps; when that app gains focus, its board becomes the active quick-paste set automatically (e.g. a SQL-snippets board in the DB client, an emoji/signature board in mail). _Value: Reuses the source-app metadata vbuff already captures to make the right snippets surface without manual switching - an approachable, zero-config power feature that Espanso only approximates with per-app scoping config._
211. **Quick-filter chips you can pin and a 'filter by example' action** `[S]` - Right-click any clip and choose 'show clips like this' to instantly build a filter (same app + same content type + same domain); the resulting chip set is one-click savable to the sidebar. _Value: Lower-friction than the catalogued saved-searches/prefix-operators: discovery-by-example means casual users get power filters without learning 'app:Chrome type:url' syntax, matching the approachability pillar._
212. **Self-consuming queue board for repetitive batch work** `[M]` - A dedicated board where each paste removes the item and visibly advances a progress counter (e.g. '7 of 30 license keys pasted'), with undo to re-queue a popped item. _Value: Combines consume-on-paste + queue from the catalog into one purpose-built UI for the real job (pasting a list of keys/IDs/emails one per field) with progress and undo that neither Clibor nor Keyboard Maestro surface._
213. **Capture-to-collector hotkey (append next N copies into one clip)** `[S]` - A toggle that, while active, appends each new copy onto a single growing 'collector' clip with a chosen joiner and a live count badge, ended by pressing the toggle again. _Value: The catalog's 'append-to-existing on copy' is a single-shot future item; a sustained collector mode is the natural way to gather scattered snippets (quotes from a PDF, fields across tabs) into one paste - common research/dev workflow._
214. **Action chooser ranked by destination app and content type** `[M]` - The quick-action palette reorders and pre-selects likely transforms based on the focused app and the clip's detected type (JSON clip into an editor surfaces pretty-print first; URL into a terminal surfaces strip-tracking + wget-wrap). _Value: Makes the catalogued quick-action menu feel smart instead of an alphabetical wall; reduces keystrokes for the common case and showcases polish over CopyQ's flat command list._
215. **Reversible transforms with a per-clip transform history** `[L]` - Each paste-time transform is recorded against the clip so a user can re-open a previously-transformed paste, see the chain applied, tweak one step, and re-paste - canonical bytes stay untouched as an immutable base. _Value: Honors the 'never mutate stored bytes' rule while giving the undo/iterate loop power users expect; nobody in the catalog tracks transform provenance, and it makes scripted actions debuggable._

### UI/UX & accessibility delights

216. **Velocity-aware row virtualization (text-while-scrolling)** `[M]` - During fast ScrollArea fling, render rows as cheap text-only skeletons and defer thumbnail decode/syntax-highlight until the scroll velocity drops below a threshold, then fade the rich content in. _Value: Keeps the egui hot path at 60fps on huge image/code histories where per-row thumbnail blits or highlight passes would otherwise stutter; the catalog has virtualization and thumbnails but nothing about velocity-gated rendering. Directly defends the polish wedge._
217. **Type-to-paste digit overlay with home-row option** `[S]` - When the popup opens, fade a translucent 1-9 (then a-z home-row) badge onto each visible row so the user can paste the Nth item by one keystroke without arrowing, with the labels reflowing live as the filter narrows results. _Value: The catalog has 'digit quick-pick' as a bare feature, but the live-relabeling overlay that tracks filtered results is the delight; it makes the popup a launcher-grade muscle-memory tool and reduces keystrokes-to-paste to one._
218. **Diff-aware near-duplicate collapse with inline delta** `[M]` - When consecutive clips differ only slightly (e.g. you re-copy a URL after editing a query param), collapse them into one row showing the latest plus a subtle inline highlight of what changed, expandable to the full chain. _Value: Heavy copy-edit-recopy workflows (devs, writers) flood history with 90%-identical entries; this declutters the list visually without losing versions, and the inline delta is a genuinely novel preview affordance not in the 640-item catalog._
219. **Confidence-shaded fuzzy match highlighting** `[S]` - Instead of binary bold-on-match, shade each matched character's highlight opacity by the fuzzy matcher's per-character score so strong contiguous matches read darker than scattered weak ones. _Value: Turns the match highlight into a scannable ranking cue: users instantly see why the top result won and whether to keep typing; extends catalogued 'live match highlighting' with information the matcher already computes but normally throws away._
220. **Spoken paste-confirmation echo for screen-reader users** `[S]` - After a paste-back completes, emit an accesskit live-region announcement of what was pasted and where ('Pasted 142 characters into Safari, plain text'), gated behind the OS screen-reader-active signal. _Value: Paste-back is invisible to a blind user once the popup closes; an explicit success echo closes the loop and distinguishes a real paste from the catalogued silent-failure trap. Goes beyond the catalogued 'announce clip metadata in rows' which only covers browsing._
221. **Caret-anchored popup with smart flip and arrow tail** `[M]` - Place the popup at the text caret (not just the mouse cursor) using the per-OS caret-bounds APIs, draw a small connector tail pointing at the insertion point, and flip above/below based on remaining work-area space. _Value: The catalog lists 'placement at native cursor/caret' as a flat v2 feature; the delight is the connecting tail plus space-aware flip that makes the popup feel spatially tied to where text will land, like an autocomplete rather than a floating window._
222. **Reduced-motion crossfade fallback that still conveys state** `[S]` - When the OS requests reduced motion, replace slide/scale animations not with an instant cut but with a sub-100ms opacity crossfade, so state changes (open, filter, paste) remain perceptible without vestibular-triggering movement. _Value: Catalogued 'honor reduced-motion' typically means disabling animation entirely, which removes useful change-blindness cues; a motion-free-but-not-jarring fallback is an accessibility nicety that keeps orientation for everyone._
223. **Per-row freshness decay tinting** `[S]` - Subtly cool the row background tint as a clip ages (warm/bright for last few minutes, neutral for older), giving a glanceable recency gradient without an explicit timestamp column. _Value: Recency is the default sort but currently invisible per-row; a temperature gradient lets users feel 'how far back' they're scrolling at a glance and is a lightweight, novel density-friendly cue distinct from catalogued per-type styling._
224. **Sensitive-clip blur-until-hover (peek-to-reveal)** `[M]` - Render clips flagged as secret-like (matched concealment patterns, password-manager source) as blurred/redacted rows in the list, revealing the content only on deliberate hover or a peek key, never in passing. _Value: Shoulder-surfing protection that fits the private-by-default positioning; the catalog has concealment/exclusion (don't store) but nothing for the kept-but-shielded display case, which is the common reality for things like 2FA codes you do want briefly._
225. **Live transform preview rail in the popup** `[L]` - Show a horizontal strip of one-tap transform chips (plain text, trim, case, JSON-pretty, decode) under the highlighted clip, each rendering a tiny live before/after micro-preview of its result on that specific clip. _Value: Folds the transform pipeline into the recall flow with zero context switch; catalogued 'preview transform result before paste' is a single modal step, whereas an always-visible micro-preview rail makes transforms discoverable and reversible at a glance, advancing the 'no scripting needed' bet._
226. **First-run animated hotkey 'ghost press' coachmark** `[S]` - On first launch, render a translucent animated keycap glyph of the configured summon hotkey that gently pulses in the tray-anchored coachmark until the user successfully triggers the popup once, then dismisses itself forever. _Value: The single biggest onboarding failure for clipboard managers is users never learning the hotkey; an in-context animated cue tied to actual first success is far stickier than the catalogued static onboarding tour, and self-retires so it never nags._
227. **Keyboard-reachable per-row action menu with mnemonic flyout** `[M]` - Pressing a single key (e.g. period or Tab) on the highlighted row opens an inline action flyout where every action carries an underlined mnemonic letter, so pin/delete/transform/note are all one-then-one keystroke without arrow hunting. _Value: Catalog has individual keyboard shortcuts (pin toggle, preview) but no unified discoverable per-row command surface; the mnemonic flyout teaches its own shortcuts and keeps the whole flow on home row, a Raycast-grade ergonomic the rivals lack cross-platform._
228. **Adaptive density that auto-fits the work area** `[M]` - Offer an 'auto' density that picks compact vs comfortable based on available vertical work-area height and DPI at popup-open time, so a laptop shows enough rows while a 4K display gets larger comfortable previews, without a manual toggle. _Value: Catalogued density is a static user toggle; auto-fit removes a settings decision and makes the popup feel right on every machine the cross-platform user roams to, reinforcing the 'same tool everywhere, always sized well' wedge._
229. **Color-clip swatch with contrast-pair and copy-format ring** `[M]` - For detected color clips, render the swatch with a ring segmented into the available formats (hex/rgb/hsl) selectable by arrow key, plus an auto-computed black/white legibility dot showing which text color is readable on it. _Value: Goes well past catalogued 'detect and preview color values': designers get format choice and an instant a11y contrast hint inline, turning a passive preview into an actionable, accessible picker unique among clipboard managers._
230. **Focus-loss grace period with visual 'pinning' affordance** `[S]` - On focus loss, instead of instantly dismissing, briefly dim the popup and show a thin progress edge counting down ~400ms; clicking back or pressing a key cancels dismissal and visibly 'pins' it, preventing accidental loss when alt-tabbing to check the target. _Value: Catalogued behavior is binary dismiss-on-focus-loss vs a separate pin-open mode; the grace period plus visible countdown resolves the real friction of losing your place when you glance away, a small delight that prevents the frustrating instant-close._
231. **Search field empty-state with rotating contextual hints** `[S]` - When the search box is empty, cycle subtle placeholder hints drawn from real capabilities the user hasn't used yet ('type to filter', 'press ? for shortcuts', 'paste plain with Shift+Enter'), advancing only between sessions so they're learnable not flickery. _Value: Progressive feature discovery without a tour or docs; the catalog has a static cheat-sheet overlay but nothing that surfaces unused features in the natural idle moment, raising the ceiling of features casual users actually find._
232. **High-contrast and forced-colors theme self-audit badge** `[M]` - Bundle a build-time WCAG check (catalogued) plus a runtime self-test that, when the OS forced-colors/high-contrast mode is active, verifies the resolved egui palette still meets AA and shows a quiet settings badge if a user accent override has broken it. _Value: Custom accent colors (a catalogued feature) routinely break contrast; a live self-audit that warns the specific user when their chosen accent fails in their current OS contrast mode prevents silently-unreadable UI, going beyond build-time-only validation._

### Everyday quality-of-life features

233. **Near-duplicate (fuzzy) dedup for whitespace/wrapping variants** `[M]` - Beyond exact BLAKE3 hash matching, compute a secondary normalized fingerprint (trim, collapse internal whitespace, normalize line endings/wrapping) so a clip re-copied with only spacing or line-wrap differences merges into the existing entry instead of creating a near-twin, while the canonical bytes are still stored byte-for-byte. _Value: The catalog only dedups exact byte matches; real history fills with cosmetic variants of the same URL/snippet copied from differently-wrapped sources. Surfacing a 'this is a variant of an earlier clip' merge keeps history clean without sacrificing the byte-fidelity guarantee._
234. **Dedup 'merge ledger' so re-copies build a frecency signal** `[S]` - When dedup bumps an existing row to the top, also increment a copy-count and stamp last-copied time on it, then expose those counts as a frecency boost in default ordering and as a 'copied N times' badge. _Value: vbuff already bumps the timestamp on a hash match but throws away the fact that you keep re-copying the same thing; turning that discarded signal into frecency means your habitual clips float up for free, addressing the 'I keep scrolling for the thing I copy daily' pain without manual pinning._
235. **Auto-favorite candidates (suggested pins)** `[M]` - Detect clips that cross a usage threshold (copied or pasted N times in a window) and surface a dismissible 'Pin this?' affordance on the row, with a one-key accept, instead of requiring the user to notice and pin manually. _Value: Favorites in the catalog are purely manual; most users never curate. Proactively nominating the few clips that have earned permanence converts the most-reused transient items into a persistent bank with near-zero effort, the core of 'never lost'._
236. **Grace-bin (soft delete) with short undo window** `[M]` - Items removed by eviction, clear-all, or manual delete drop into an encrypted grace-bin for a short configurable window (e.g. 5 minutes / last 50 deletions) with one-key 'Undo last delete', rather than being scrubbed immediately. _Value: Silent data loss is the worst-category failure in the project's own mistakes doc; today clear-all and FIFO eviction are irreversible. A bounded, still-encrypted grace-bin makes accidental loss recoverable without weakening secure-delete (the bin self-scrubs on the same window)._
237. **Smart retention by content kind, not one global age** `[M]` - Let auto-expiry differ per detected kind: keep code/snippets/links long, expire bulky images and one-off pastes fast, with sensible defaults shipped (e.g. images 7d, text 30d, detected-secret 1h) rather than a single global age applied to everything. _Value: The catalog has one global retention age plus a separate shorter expiry only for 'sensitive' types; tuning by kind keeps the valuable long tail (snippets) while stopping screenshot bloat, matching how people actually value different clip types and keeping the DB lean automatically._
238. **Idle/away-aware auto-pause of capture** `[M]` - After a configurable idle period or screen-lock, automatically pause capture (and optionally clear transient history on unlock) so clipboard activity from background jobs, automated paste-ins, or another user at your unlocked machine isn't silently recorded. _Value: Existing pause/incognito are manual one-shots; tying capture to presence is a sensible default that quietly reduces noise and shrinks the window where secrets land in history, reinforcing private-by-default without the user remembering to toggle anything._
239. **Per-app rule auto-suggestions from observed behavior** `[M]` - Watch for patterns (e.g. you always 'paste as plain text' into your terminal, or copies from your password manager keep getting captured) and proactively offer to create the matching per-app paste/capture rule with one click. _Value: Per-app rules exist in the catalog but require users to know they want them and configure a rules table; learning the rule from repeated manual actions turns a power-user feature into an approachable, self-configuring one, which is the project's stated approachability bet._
240. **Capture-health surfaced as actionable notifications, not just a tray glyph** `[S]` - When the watchdog detects capture stalled, Accessibility/permission dropped, or a GNOME-Wayland degraded mode, fire a single actionable OS notification that deep-links to the fix, plus a 'last successful capture: Xm ago' freshness line in the popup header. _Value: The architecture already tracks a capture-health state but only as a passive tray indicator; users don't watch the tray. An active, debounced alert when capture silently dies directly attacks the worst-category failure and the macOS-permission-drops mistake, turning invisible breakage into a fixable prompt._
241. **Search shortcut: instant 'kind' jump-keys in the empty query** `[S]` - With the search box empty, single keypresses jump-filter to a kind (u=URLs, i=images, c=code, f=files, #=colors) and Tab cycles kinds, so common slices are one keystroke away without typing prefix operators like type:image. _Value: The catalog offers type chips (mouse) and typed prefix operators (verbose); a single muscle-memory key for the handful of kinds people actually filter by is faster and more discoverable, fitting the keyboard-first hot path that is Bet 1._
242. **Sticky last-used filter scope with a visible 'clear scope' escape** `[S]` - Remember the filter/scope you last left the popup in (e.g. 'images only', 'this collection') and reopen there, but always show a prominent one-key reset so you're never confused by a silently pre-filtered list. _Value: Most clipboard pickers reset to all-history every open, forcing re-filtering; persisting scope is a real time-saver, but the catalog's smart filters don't address reopen behavior. The visible escape hatch avoids the classic 'why is my list empty' trap that sticky filters usually cause._
243. **First-run sensible-defaults profile picker** `[S]` - On first launch, offer 2-3 one-click presets (Casual: 200 items / 30d / password managers excluded; Developer: 2000 items / code retained long / strip-tracking-params on; Privacy-max: 50 items / clear-on-lock / aggressive secret expiry) instead of dropping users into a blank settings panel. _Value: The project bets on zero-config approachability, but a single hardcoded default fits no one well; a tiny preset chooser sets retention, exclusions, and protections sensibly in one click while still revealing depth later, dramatically improving the out-of-box experience._
244. **Quiet weekly 'clipboard health' digest** `[M]` - A dismissible, opt-in periodic summary inside the app (not a nagging notification) showing DB size and trend, largest items, oldest clips about to expire, items copied 10+ times that aren't pinned, and any captures skipped by secret detectors, with inline prune/pin actions. _Value: The catalog has a static storage dashboard and a diagnostics command; framing it as a low-frequency digest with one-click cleanup and pin-suggestions turns maintenance into a 30-second habit, keeps the store lean, and quietly demonstrates that privacy protections are actually firing._
245. **Auto-collapse copy bursts into a single grouped entry** `[L]` - When several distinct clips are captured within a few seconds from the same source app (multi-cell copy, rapid copying down a list), group them under one expandable history entry that can be pasted individually or as a merged block, instead of flooding the top of the list. _Value: The catalog debounces duplicate change-events for one copy but does nothing for legitimately distinct rapid copies, which bury everything else; grouping keeps the list scannable and doubles as a lightweight, automatic paste-stack for the common spreadsheet/list workflow._
246. **Per-clip 'protect' lock that exempts an item from every automatic action** `[S]` - A one-key per-item lock (distinct from pin/favorite) that exempts a clip from expiry, eviction, near-dup merging, auto-clear, and clear-on-lock, so a value you'll need later in the session survives even aggressive privacy defaults without promoting it to a permanent favorite. _Value: Pins exempt from eviction, but users often want a temporary 'don't let anything eat this' guard that isn't a curated favorite; this gives confidence to run tight retention/clear-on-lock defaults (the privacy bet) without fear of losing the one clip that matters right now._
