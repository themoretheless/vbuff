# Implementation batch 201-250

Reviewed on 2026-07-20. This ledger is the execution overlay for backlog items 201-250 in [ideas-top-300.md](ideas-top-300.md). It keeps runtime UI, pure workflow contracts, plugin-host foundations, native placement dependencies, and store lifecycle behavior separate.

## Status vocabulary

| Status | Meaning |
|---|---|
| **Runtime** | Exercised by the current resident binary, popup, store, or app command path. |
| **Foundation** | Implemented and tested as a bounded reusable contract, but not connected to the final runtime surface. |
| **Adapted** | The proposal was narrowed to preserve privacy, accessibility, or a truthful product boundary. |
| **Native required** | Completion depends on real per-OS APIs or assistive-technology evidence. |
| **Rejected** | The mechanism conflicts with a safety or correctness constraint; its replacement is recorded. |

## Item ledger

| Item | Status | Landed evidence | Remaining product work |
|---:|---|---|---|
| 201 | Foundation | [`compare_text`](../crates/vbuff-core/src/workflow/compare.rs) produces bounded line/word chunks through `similar` without modifying either input. | Connect two-row selection, merged-copy policy, and accessible diff rendering to the popup. |
| 202 | Runtime | [`TransformOverlay`](../crates/vbuff-core/src/workflow/compare.rs) hashes canonical input and exposes immutable output; the popup preview rail uses it for trim, uppercase, and JSON before `PasteText`. | Persist an optional redacted transform receipt only after lifecycle and consent policy are defined. |
| 203 | Foundation | [`SnippetForm`](../crates/vbuff-core/src/workflow/snippets.rs) validates typed fields and evaluates bounded checkbox/dropdown visibility predicates. | Build the GUI authoring surface and durable snippet schema. |
| 204 | Foundation | Computed fields support slugify, uppercase, character count, and bounded date offsets with dependency ordering and cycle rejection. | Add localized date presentation and authoring UX without introducing a scripting language. |
| 205 | Foundation | [`PipelineBuilder`](../crates/vbuff-plugin/src/pipeline.rs) inserts, removes, reorders, type-checks, and exports a node/edge graph. | A visual editor waits for the reviewed plugin host and signed installation path. |
| 206 | Foundation | [`ActionPermissionRequest`](../crates/vbuff-plugin/src/manifest.rs) and `ActionCapabilityGrant` bind action id, manifest hash, capabilities, paths, hosts, environment keys, and timeout by exact match. | Enforce grants inside a real sandboxed host; no shell or external action runs today. |
| 207 | Foundation | `dry_run_explained` returns bounded output plus ordered, redacted per-step type/size explanations from the same pipeline implementation. | Surface the explanation in GUI/CLI and compare it with committed execution once a host exists. |
| 208 | Foundation | [`RangeSelection`](../crates/vbuff-core/src/workflow/selection.rs) preserves visible order and `SelectionAggregate` bounds count, bytes, kinds, and merged preview. | Wire shift/drag selection and aggregate actions into virtualized rows. |
| 209 | Foundation | [`PinBoard`](../crates/vbuff-core/src/workflow/boards.rs) owns stable slots 1-9 with bounded labels and replacement/removal semantics. | Add persisted boards and grid presentation after collection ownership is settled. |
| 210 | Foundation | `BoardRouter` resolves validated app/window/project matchers in deterministic priority order with redacted diagnostics. | Obtain trustworthy native focused-window context and add explicit override UX. |
| 211 | Foundation | `filter_from_example` derives kind, app, source host, and bounded tags without query-string construction. | Add the row action and filter editor once tags/source metadata are active in the runtime. |
| 212 | Foundation | `ConsumeQueue` advances one item at a time, reports progress, and supports one-step undo. | Bind consume to acknowledged paste completion rather than key injection intent. |
| 213 | Foundation | `CaptureCollector` accepts a bounded target count and reviewed newline/space/tab/custom joiners with output limits. | Add a capture-mode command and visible cancellation without bypassing the privacy gate. |
| 214 | Foundation | `rank_actions` applies deterministic kind and destination-app affinity with stable tie-breaking. | Feed verified destination identity and measure choices locally before learning weights. |
| 215 | Foundation | `TransformHistory` stores bounded transform descriptors, hashes, sequence ids, and replayable output while redacting text from `Debug`. | Define encrypted persistence and user-visible pruning before retaining history across restarts. |
| 216 | Runtime | The existing Compose paste stack is active; [`SessionBasket`](../crates/vbuff-core/src/workflow/boards.rs) adds unique ids and explicit promotion semantics. | Persist only explicitly promoted baskets; temporary contents remain process-local by design. |
| 217 | Foundation | `Checklist` tracks ordered clip ids, done state, paste-to-check policy, and progress. | Add a working-set UI and paste acknowledgement integration. |
| 218 | Foundation | `NamedSlots` binds one-shot A-Z keys to stable clip ids; Compose form slots already preserve explicit one-step-at-a-time paste. | Add transient history-slot UI without automatic focus advance. |
| 219 | Foundation | [`timeline_buckets`](../crates/vbuff-core/src/workflow/timeline.rs) builds bounded hour/day/session ranges with stable clip indices. | Add the scrubber control and store query that pages around a selected bucket. |
| 220 | Foundation | `group_work_sessions` combines bounded idle gaps with app/project context while preserving source clip ids and order. | Persist project metadata and add accessible collapse/expand state. |
| 221 | Runtime | [`ScrollTuner`](../crates/vbuff-gui/src/experience.rs) measures velocity and the virtualized popup takes a cheap row path while scrolling rapidly. | Add measured thumbnail decode cancellation under real large-image workloads. |
| 222 | Runtime | Number labels appear only while the command modifier is held and always map to the current top nine filtered rows. | Validate layout-independent number-row behavior on all native keyboard backends. |
| 223 | Runtime | `NearDuplicateDelta` conservatively collapses adjacent text variants, shows changed prefix/suffix counts, and permits explicit expansion; schema 6 adds durable normalized groups. | Tune grouping against multilingual corpora and never auto-delete variants. |
| 224 | Runtime | Query highlights use bounded score-derived alpha through `match_highlight_alpha` instead of a binary style. | Calibrate against relevance fixtures and forced-color modes. |
| 225 | Runtime | Successful/failed paste publishes a content-free message into a polite AccessKit live region with monotonically increasing revision. | Manual NVDA, VoiceOver, and Orca verification remains an external gate. |
| 226 | Native required | [`PopupAnchor::Caret`](../crates/vbuff-platform/src/geometry.rs) places below/above a caret rect and safely falls back to cursor/work-area clamping. | Implement native caret-bound acquisition per OS and prove focus is not stolen. |
| 227 | Adapted | Reduced-motion mode removes the 120 ms preview transition entirely instead of retaining a motion effect against the user's preference. | Verify with OS reduced-motion settings once native preference plumbing exists. |
| 228 | Runtime | `recency_strength` applies a restrained freshness tint while keeping text contrast independent. | Tune thresholds with theme and forced-color evidence. |
| 229 | Runtime | Sensitive rows remain masked and support a deliberate two-second hover/focus peek that is cleared on hide. | Bind reveal to native screen-share state and stronger user policy later. |
| 230 | Runtime | The selected row's side pane exposes Original, Trim, Uppercase, and JSON transform previews and pastes derived text without rewriting history. | Add richer transforms through the same immutable boundary. |
| 231 | Runtime | A persisted one-time summon-key coachmark appears until explicitly dismissed; command wiring updates config atomically. | Add tray anchoring where the native tray API exposes reliable geometry. |
| 232 | Runtime | The row action flyout exposes Paste, Pin, Peek/Add, Preview, Transform, and Delete with mnemonic keys. | Validate mnemonic conflicts under localized labels and non-QWERTY layouts. |
| 233 | Runtime | `DensityMode` selects bounded compact/comfortable row heights from logical viewport and DPR; web breakpoints use CSS logical dimensions. | Persist the preference and validate native monitor transitions. |
| 234 | Runtime | Color rows render a three-segment HEX/RGB/HSL ring, swatch, output conversion, and contrast hint. | Add alpha-aware contrast and a destination color-format preference. |
| 235 | Runtime | `FocusLossGuard` gives a 700 ms dimmed countdown, immediate refocus recovery, and deterministic tests. | Validate against native popup activation edge cases on each OS. |
| 236 | Runtime | Empty search hints rotate from actual unused capabilities and remain restrained placeholder text. | Add local-only dismissal/usage state rather than telemetry. |
| 237 | Runtime | Settings computes the resolved foreground/panel contrast ratio and reports AAA, AA, or FAIL with non-color text. | Listen to native forced-colors changes and audit every semantic token, not only the primary pair. |
| 238 | Runtime | Settings includes Arabic, Hebrew, Japanese, Simplified Chinese, and Korean samples from one typed gallery. | Add real shaping/font-fallback diagnostics and reviewed screenshots per platform. |
| 239 | Adapted | Settings uses native egui focus order, semantic controls, and command-palette access; no pointer-only operation is required. | Manual assistive-technology and non-QWERTY mnemonic evidence remains required. |
| 240 | Runtime | `MotionBudget` records frame duration and dropped-frame count; an opt-in overlay shows frame, scroll, and transition budgets. | Add repaint-cause attribution after upstream egui exposes a stable hook. |
| 241 | Runtime | Optional left-hand Alt-Q/W/E and right-hand Alt-I/O/P navigation/paste bindings preserve defaults. | Add remapping and conflict detection rather than expanding hard-coded sets. |
| 242 | Runtime | A 300-point side preview appears on wide viewports and automatically collapses on compact layouts; text, image, color, badges, and transforms share it. | Add HTML sanitization/rendering before showing rich web content. |
| 243 | Runtime | Rows and previews expose lossless/incomplete, sensitive, local-only, and expiring badges through typed `ClipBadge` values. | Add synced/degraded states only when live transport/native fidelity can prove them. |
| 244 | Runtime | Pin, paste-stack add, and confirmed delete all create a five-second icon Undo; deleted payload is redacted in commands, held only in memory, and cleared when the popup hides. | Durable post-restart recovery is item 249 and remains OS-key-provider gated. |
| 245 | Runtime | Cmd/Ctrl-K opens a searchable internal command palette for views, paste, pin, stack, peek, pause, preview, and diagnostics. | Add plugin commands only after permission-safe host integration. |
| 246 | Runtime | Schema 6 stores a domain-separated normalized text fingerprint; [`near_duplicate_group`](../crates/vbuff-store/src/lib.rs) returns byte-distinct variants and never overwrites canonical flavors. | Evaluate Unicode normalization and collision thresholds on a multilingual corpus before automatic collapsing. |
| 247 | Runtime | Every exact hash re-copy appends a timestamped `dedup_merge_ledger` event; count and ordered history are queryable. | Add bounded compaction if real histories make per-event storage material. |
| 248 | Foundation | `suggested_pins` ranks non-sensitive, unpinned rows by exact reuse count and stable recency. | Add explain/dismiss UX and frecency decay before presenting suggestions. |
| 249 | Foundation | `delete_with_grace` hydrates CAS first, seals the complete clip with XChaCha20-Poly1305 and metadata-bound AAD, stores no key/plaintext, restores only after identity/hash verification, and securely purges expiry. | Supply the key from an OS keystore and wire runtime delete/eviction policy; plaintext or a key beside the DB is rejected. |
| 250 | Foundation | Persisted validated rules independently bound age/count/grace for all nine kinds plus a hard-delete sensitive override; enforcement is capped and defers grace deletions when no key exists. | Add settings/impact preview and call maintenance only after item 249's key provider is available. |

## Three review passes

| Pass | Focus | Corrections made before acceptance |
|---|---|---|
| 1 | Bounds, privacy, and state ownership | Added size/count/identifier limits throughout workflows; made transformed and snippet values content-redacted in diagnostics; bound action grants to exact manifest/action/scope data; preserved every near-duplicate's canonical bytes; made sensitive rows ineligible for normalized groups and pin suggestions; hydrated CAS before grace encryption; authenticated recovery metadata; verified restored identity/hash; deferred non-zero grace retention when no key exists. |
| 2 | Visual design, accessibility, and responsive behavior | Added one restrained action menu, stable icon controls/tooltips, adaptive density, wide preview, compact collapse, match confidence, recency, peek, color ring, Settings, command palette, and live announcements. The later native-only review removed the browser timing/CSS compatibility layer; `eframe` owns native DPI scaling without an artificial zoom override. |
| 3 | SOLID/DRY, migration, and claim accuracy | Split pure workflow behavior into `vbuff-core::workflow`, presentation metrics into `vbuff-gui::experience`, permission/pipeline contracts into `vbuff-plugin`, placement into `vbuff-platform`, and lifecycle/crypto policy into `vbuff-store::lifecycle`. Replaced an eight-argument decrypt boundary with one typed encrypted record; continued the bounded normalized-fingerprint migration in idle maintenance and marked unusable legacy text as scanned; added schema-5-to-6 preflight coverage and a v2 contract instead of rewriting v1; kept caret acquisition, plugin execution, real screen-reader evidence, and OS-keystore integration explicitly open. |

## Acceptance gate

The active baseline requires formatting, strict all-target/all-feature clippy, locked workspace tests, root no-default-feature tests, store disk/CAS recovery tests, native GUI goldens, documentation contracts/local links, workflow review, and `git diff --check`. Native caret bounds, real assistive-technology runs, OS-keystore key delivery, plugin sandbox execution, and forced-color/native monitor evidence remain separate product gates after deterministic code is green.

Post-merge CI review on 2026-07-21 regenerated and visually reviewed all 16 light/dark, 1x/2x Linux popup goldens, then reproduced an exact no-update match on Ubuntu 24.04 with Rust 1.97.0. The same correction added version-pinned `cargo-vet` coverage for `similar`/`bstr` and synchronized `fuzz/Cargo.lock`; locked vetting and fuzz-target compilation passed locally before the follow-up push.
