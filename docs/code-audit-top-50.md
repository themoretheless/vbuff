# vbuff - Top 50 Things Done Wrong in the Current Implementation

> This audits the **actual code in this repository** (`crates/`, `src/`), not competitors. It is the counterpart to
> [`docs/mistakes-top-500.md`](mistakes-top-500.md) (competitor anti-patterns) and exists because `README.md`,
> `architecture.md`, and `recommendation.md` describe a mix of shipped and target-state behavior in the same
> present-tense voice, making it hard to tell what actually exists. Every item below is grounded in a specific file
> and line, checked against the corresponding claim in the docs. Severity: 🔴 critical · 🟠 high · 🟡 medium · 🟢 low.

## Table of contents

1. [Privacy & security gaps](#1-privacy-security-gaps) (10)
2. [Capture fidelity & platform-parity gaps](#2-capture-fidelity-platform-parity-gaps) (10)
3. [Storage, search & eviction inconsistencies](#3-storage-search-eviction-inconsistencies) (8)
4. [Concurrency, robustness & performance](#4-concurrency-robustness-performance) (12)
5. [Process & lifecycle gaps](#5-process-lifecycle-gaps) (5)
6. [Documentation hygiene](#6-documentation-hygiene) (5)

---

## 1. Privacy & security gaps

1. **No encryption at rest anywhere** `[critical]` - `crates/vbuff-store/src/lib.rs` opens a plain `rusqlite::Connection` (`Cargo.toml` pins `rusqlite` with only the `bundled` feature, no `sqlcipher`). README's "Encrypted at rest with the key held in the OS secret store" and recommendation.md's Bet 3 are both false for the shipped binary: the history DB, including every password or token ever copied, sits on disk in plaintext SQLite.
2. **No OS secret-store integration** `[critical]` - no `keyring` (or any) crate is a dependency anywhere in `Cargo.toml`/`Cargo.lock`; there is no key of any kind to hold, since there is no encryption (#1).
3. **Concealed/transient pasteboard hints are never checked** `[critical]` - nothing in `crates/vbuff-platform/src/clipboard.rs` reads `org.nspasteboard.ConcealedType`/`TransientType`, Windows `ExcludeClipboardContentFromMonitorProcessing`/`CanIncludeInClipboardHistory`, or KDE `x-kde-passwordManagerHint`. `arboard` doesn't expose any of these, and nothing else was added. Password-manager secrets are captured like any other text.
4. **The app-exclusion feature is dead code in practice** `[critical]` - `ArboardClipboard::read()` (`clipboard.rs:63-66`) always sets `source_app: None`. `main.rs:150-155` only calls `config.is_excluded(app)` inside `if let Some(app) = &captured.source_app`, which never matches. `excluded_apps` is fully implemented, unit-tested (`config.rs:112-121`), and completely inert at runtime.
5. **No default secret-tool deny-list ships** `[high]` - `Config::default()` (`config.rs:30-41`) sets `excluded_apps: Vec::new()`. README promises "Ship a sane default deny-list"; the default is empty, and would be inert anyway (#4).
6. **No secret detection, no regex/keyword rules** `[high]` - grep confirms no regex-based content rule or secret-pattern detector exists anywhere in `vbuff-core` or elsewhere, despite being listed in README's "Private and trustworthy by construction" section.
7. **No incognito mode** `[high]` - `Config` (`config.rs:12-28`) has no incognito field, and no code toggles a no-persist mode; only `paused` (full stop) exists. README lists "incognito mode" as a shipped MVP feature.
8. **No manual capture-on-demand hotkey** `[medium]` - only one hotkey is ever registered (`main.rs:75-84`, the show/hide combo). README claims a second, "manual capture-on-demand hotkey" ships.
9. **No master password, idle auto-lock, auto-clear-on-timer, or wipe-on-demand** `[high]` - none of these exist in `Config` or `main.rs`; README's privacy bullet list names all four as present.
10. **No accessibility-permission onboarding** `[medium]` - README promises "the onboarding flow deep-links you to the right settings pane" for macOS Accessibility; the actual behavior is a single `tracing::warn!("paste backend unavailable; paste-back disabled")` log line (`main.rs:219-221`) with no UI at all.

## 2. Capture fidelity & platform-parity gaps

11. **`arboard` polling is used on every platform, which recommendation.md itself calls disqualifying** `[high]` - recommendation.md's own "Build now" table says arboard "is the wrong tool (no events, single-flavor)" and calls native event-driven capture "non-negotiable" (section 2, row 1; also step 4 of "the 10 things"). The shipped `main.rs:104-186` polls `arboard` on a fixed timer on macOS, Windows, and Linux alike - exactly what the document tells the team not to ship.
12. **Fixed 300ms poll with single-hash comparison misses rapid successive copies** `[high]` - `spawn_capture_thread` (`main.rs:110-135`) sleeps the full interval, reads once, and skips if the hash matches `last_hash`. Any copy that happens and is overwritten between polls is lost - this is items #1 and #3 from vbuff's own `docs/mistakes-top-500.md`, reproduced in vbuff.
13. **No idle backoff; poll runs at a constant rate forever** `[medium]` - the loop in `main.rs:122-124` never varies `interval`, contradicting both README's "idling near 0% CPU" claim and the adaptive-poll behavior architecture.md prescribes for macOS `changeCount`.
14. **Only one flavor is ever captured per copy, never several at once** `[high]` - `ArboardClipboard::read()` (`clipboard.rs:37-66`) tries text, and only attempts an image read `if flavors.is_empty()`. A web copy that offers HTML + plain text + image is stored as a single flavor, not the "captures all flavors of a single copy atomically" README promises.
15. **No RTF, HTML, or file/uri-list capture at all** `[high]` - `arboard`'s API surface used here is `get_text`/`get_image` only; there is no code path that reads `text/html`, RTF, or file lists. README's "Stores plain text, rich text/HTML, RTF and images out of the box" is true only for text and (single) images.
16. **`source_app` is never populated** `[high]` - `CapturedClipboard { flavors, source_app: None }` is hardcoded in `clipboard.rs:63-66`. Per-app attribution, the meta line's app badge, and search-by-source-app are all permanently inert, and this is also the direct cause of #4.
17. **No native X11/XFIXES or Wayland/`wlr-data-control` backend exists** `[high]` - `vbuff-platform` has exactly one `ClipboardBackend` impl (`ArboardClipboard`). README's platform-support table describing per-OS native capture (`XFIXES` selection events, `wlr-data-control`, `AddClipboardFormatListener`) documents backends that do not exist in this repo.
18. **No GNOME-Wayland capture-on-summon fallback** `[medium]` - README's detailed "GNOME on Wayland caveat" paragraph describes a fallback and an in-app explanation; neither exists anywhere in the code.
19. **`global-hotkey`'s own doc comment says it doesn't cover Wayland, yet nothing else fills the gap** `[medium]` - `crates/vbuff-platform/src/hotkey.rs:1-5`: "`global-hotkey` covers macOS, Windows, and Linux/X11 (not Wayland)." README documents a working `GlobalShortcuts`-portal path for Wayland that is not implemented anywhere.
20. **`Body::Spilled` is fully dead code** `[medium]` - the variant exists in `vbuff-types/src/lib.rs:129-134` for "large payloads spilled to an out-of-row content-addressable file," but nothing in the codebase ever constructs one; every capture is stored `Body::Inline` regardless of size, with no per-item cap.

## 3. Storage, search & eviction inconsistencies

21. **No FTS5 table exists, despite an explicit claim that it does** `[high]` - `Store::migrate()` (`crates/vbuff-store/src/lib.rs:71-96`) creates only the `clips` table plus three plain B-tree indexes. recommendation.md step 2 of "the 10 things" states as settled fact: "FTS5 schema present (substring is the shipped tier)." It is not present.
22. ~~**`Store::search()` is unreachable dead code in the running app**~~ `[medium]` **[FIXED]** - the dead `LIKE`-based method and its tests were deleted; `vbuff_core::filter::search` (already the live path) remains the only search implementation. A real SQL-backed search returns when FTS5 lands (v1).
23. ~~**Two independent, divergent search implementations exist for the same feature**~~ `[medium]` **[FIXED]** - resolved as a side effect of #22: only one search implementation exists now.
24. **`vbuff_core::eviction::evict` is dead code; eviction is reimplemented separately in raw SQL** `[medium]` **[MITIGATED, not merged]** - kept as two implementations on purpose (SQL avoids loading full `Clip` rows just to compute a cap), but a new test (`enforce_cap_matches_pure_eviction_policy` in `vbuff-store/src/lib.rs`) now asserts they agree on every eviction decision. Writing that test immediately caught a real, live bug: `evict()` was sorting by `ClipId` (fixed at first capture) instead of last-touched time, so a clip re-copied a second ago could be evicted ahead of one nobody had touched in weeks. Fixed by adding `ClipMeta::updated_at` (did not exist before) and sorting by it.
25. **Binary payloads are JSON-array-of-numbers encoded** `[medium]` - `serde_clip.rs` serializes `Vec<u8>` flavor bodies via `serde_json`, which the file's own comment admits is "bulky" (roughly 3-5x raw size for images). This directly feeds the unbounded-DB-growth failure mode `docs/mistakes-top-500.md` items 42/44 call out as critical for competitors.
26. **`GUI_LIMIT` (1000) is unrelated to `config.max_history`** `[medium]` - `main.rs:38` hardcodes `GUI_LIMIT: usize = 1000` independent of the user-configurable `max_history` (`config.rs:20`, `35`). If a user raises `max_history` above 1000, everything past the first 1000 by recency becomes permanently unsearchable and unreachable from the popup, with no indication why.
27. **No per-item size cap on capture** `[medium]` - nothing in `spawn_capture_thread` (`main.rs:104-186`) bounds the size of a captured flavor; an arbitrarily large text blob or image is stored fully inline (see #20/#25).
28. ~~**Search only ever matches the 512-char cached preview, not full content**~~ `[low]` **[FIXED]** - moot as of #22: the `Store::search` path this described no longer exists. Worth re-checking once a real SQL/FTS5 search path is built.

## 4. Concurrency, robustness & performance

29. **The entire clip list is cloned every single frame while the popup is visible** `[high]` - `PopupApp::update` (`crates/vbuff-gui/src/app.rs:103-107`) does `s.clips.clone()` unconditionally on every call, including every inline image byte buffer, and `ctx.request_repaint()` at the end of `update` (`app.rs:248`) forces another frame immediately - a continuous full-list clone loop for as long as the popup stays open.
30. **Thumbnail texture cache is never pruned** `[medium]` - `PopupApp.thumbnails: HashMap<String, Option<TextureHandle>>` (`app.rs:37`, `327-337`) only grows; a texture is cached forever once created, even after its clip is deleted or evicted from history. Unbounded GPU-resource growth for the life of the process.
31. **Widespread `Mutex::lock().unwrap()` on both shared state and the store** `[high]` - e.g. `main.rs:172` (`store.lock().unwrap()`), `state.rs`/`app.rs` (`self.state.lock().unwrap()` throughout). A single panic anywhere while holding either lock poisons it, and every later `.unwrap()` on that same mutex panics too - with no supervisor or watchdog, this is the exact silent-full-stop failure mode `docs/mistakes-top-500.md` item 22 calls out as a top competitor bug, now possible in vbuff itself.
32. **Inconsistent lock-error handling in the same file** `[low]` - `main.rs`'s tray/action handlers use `if let Ok(store) = store.lock()` (silently swallowing a poisoned lock, e.g. `main.rs:292`, `400`, `406`, `412`), while the capture thread uses `store.lock().unwrap()` (`main.rs:172`, panicking on the same condition). Neither path surfaces the failure to the user; there is no "capture stopped" indicator anywhere (contradicting `docs/mistakes-top-500.md` item 36's own observability recommendation).
33. **Capture-thread read errors are silently discarded** `[medium]` - `main.rs:129-132`: `Err(_) => continue` with no logging, no backoff, no circuit breaker. A permission problem or a transient OS lock failure produces no signal to the user or the logs.
34. **No mock backends exist for any of the four platform traits** `[medium]` - recommendation.md step 1 of "the 10 things" calls for `MockBackend` implementations so `vbuff-core` (and the app glue) is "testable headless on any host." `crates/vbuff-platform` ships only real, OS-backed implementations (`ArboardClipboard`, `GlobalHotkeyBackend`, `EnigoPaste`); there is nothing to substitute in tests.
35. **Zero test coverage for real OS-facing behavior** `[medium]` - every test in `vbuff-platform` (rgba-dimension parsing, hotkey-combo string parsing) and `vbuff-gui` (hex-color parsing, relative-time buckets) exercises pure helper functions only; the actual clipboard read/write, hotkey registration, and paste-injection code paths that talk to the OS are never tested.
36. **No CI configuration exists anywhere in the repository** `[medium]` - there is no `.github/workflows` directory or any other CI config. README's "Run `cargo fmt`, `cargo clippy`, and `cargo test --workspace` before opening a pull request" is prose-only guidance, not an enforced gate.
37. **`enforce_cap`/`insert` are not wrapped in one transaction** `[low]` - `Store::insert` (`lib.rs:104-154`) and the subsequent `enforce_cap` call in `main.rs:173-178` run as three separate implicit-autocommit statements (dedup-check, insert/update, cap-enforce) rather than one atomic unit.
38. **`Config::is_excluded` is substring-only, no regex** `[low]` - `config.rs:82-88` does a flat case-insensitive substring match; README and recommendation.md both describe "regex/keyword exclusion rules" as an existing capability.
39. **Every clipboard write opens a brand-new `ArboardClipboard` handle instead of reusing one** `[low]` - `copy_latest_clip` (`main.rs:361`) and the `Paste` action handler (`main.rs:388`) both call `ArboardClipboard::new()` inline per invocation, inconsistent with the capture thread's single long-lived handle; a transient failure to open a second concurrent clipboard handle silently no-ops the write.
40. **Paste-back timing is a hardcoded, unverified 120ms guess** `[low]` - `PendingPaste { at: Instant::now() + Duration::from_millis(120) }` (`main.rs:394-396`) fires the synthetic Ctrl/Cmd+V after a fixed delay with no check that focus actually returned to the target app; on a slow-to-refocus app the keystroke can land nowhere or in vbuff's own (hidden) window.

## 5. Process & lifecycle gaps

41. **No single-instance guard exists** `[high]` - architecture.md's "Single-instance guard with handoff" is a named design goal; nothing in `main.rs` checks for or prevents a second running instance. Launching vbuff twice double-captures, fights over the same SQLite file, and races to register the same global hotkey.
42. **No autostart-on-login** `[medium]` - listed under architecture.md's Phase-1/MVP exit criteria; no code registers vbuff to start on login on any platform.
43. **No config hot-reload** `[low]` - `Config::load_or_create()` (`config.rs:56-69`) is called exactly once at startup in `main()`; changing the hotkey, poll interval, or exclusion list requires restarting the whole process, despite the stated goal of separating policy (config) from data (store).
44. **Self-write suppression is absent; paste-back triggers a full needless store round-trip** `[low]` - there is no fingerprinting of vbuff's own writes (as architecture.md's debounce/self-write section describes). Content-hash dedup in `Store::insert` prevents a duplicate row, but every paste still costs the capture thread a full insert+enforce_cap+reload cycle on its next poll for content that didn't actually change.
45. **No visible degradation/health indicator for capture, hotkey, or paste** `[medium]` - beyond scattered `tracing::warn!`/`tracing::error!` log lines, there is no UI signal anywhere (tray, popup, or otherwise) that tells the user capture stopped, the hotkey failed to register, or paste-back is unavailable - see #10, #31, #33.

## 6. Documentation hygiene

46. **`README.md`'s "Feature highlights" mixes shipped and target-only bullets in the same present tense with no consistent tag** `[medium]` - some bullets are correctly phase-tagged ("files/folders... follow in v1/v2"), while others describing entirely unbuilt behavior ("Encrypted at rest...", incognito mode, default deny-list, auto-clear-on-timer) carry no tag at all and read as already true.
47. **recommendation.md's own "non-negotiable" call is silently violated with no addendum** `[medium]` - section 2's "Build now" table rejects `arboard`-based polling outright; the shipped MVP does exactly that (#11), and no note in recommendation.md flags the deviation.
48. **recommendation.md asserts a specific schema detail as fact that isn't true** `[low]` - "FTS5 schema present" (step 2 of "the 10 things," section 2 row for the store) - see #21.
49. **No document distinguishes "in this repo today" from "target" at the bullet level for the single most safety-critical claim category (security/privacy)** `[high]` - a reader cannot tell, without reading the source, that "encrypted at rest," "honors concealed markers," and "default deny-list" are all aspirational rather than shipped.
50. **This document itself is the fix for the above four, but it must be kept in sync** `[low]` - as capture backends, encryption, and search are actually implemented, items in sections 1-3 above should be struck through or removed rather than left to silently go stale, the same discipline this audit is asking the other three documents to adopt.

---

## How to use this document

Cross-reference before quoting any "vbuff already does X" claim from `README.md`, `architecture.md`, or `recommendation.md`:
if the claim concerns capture fidelity, encryption, exclusion, or search, check here first. This file should be
updated (items struck or removed) as each gap actually closes - see item 50.
