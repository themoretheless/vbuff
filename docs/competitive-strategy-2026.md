# vbuff - Competitive Strategy Refresh (2026-07-22)

This document answers one question: **what should vbuff implement to create a durable advantage, instead of merely matching a crowded clipboard-manager market?** It supersedes the old assumption that cross-platform reach, polish, private sync, and approachability are enough by themselves.

## Method and evidence boundary

The review produced twenty accepted, independent, read-only agent reports: platform and competitor research, security, retrieval, product, fidelity, sync, UX, accessibility, performance, architecture, evidence, integrations, onboarding, adversarial product, native sequencing, and two final arbitrations. They ran in bounded waves rather than all at once. Earlier reports that inspected `tuner` instead of `vbuff` were excluded; two agents that stopped after reading only the workspace header were rerun with the full manifest. The coordinator independently checked high-impact findings and rejected one snapshot claim after all 28 generated images proved byte-identical to their baselines.

Current product claims were checked against official product pages, documentation, repositories, standards, and research available on 2026-07-22. A vendor statement proves only that the vendor makes the claim. It does not independently prove reliability, security, or performance. Product hypotheses below remain hypotheses until dogfood or user evidence passes the stated gates.

## Market reset

The previous four-corner positioning is no longer a sufficient strategy:

- macOS Tahoe 26 includes searchable clipboard history in Spotlight. Windows already includes a 25-item, 4 MB-per-item history with pinning and optional cloud sync. Generic history is now an OS feature on both major paid desktop markets.
- Paste now advertises shared pinboards, Teams, Power Search with OCR, Apple Intelligence, a local MCP server, subscription plans, and a lifetime purchase. Calling it only a polished Apple clipboard with subscription sync is stale.
- CrossPaste 2.1.6 advertises macOS, Windows, Linux, and Android support, LAN-only E2EE, rich formats, local OCR, an MCP server, and a Chrome extension. Its roadmap includes CLI and plugins.
- PowerToys Advanced Paste performs local plain-text, Markdown, JSON, file, OCR, and media transformations, with optional cloud or local-model AI.
- CopyQ already provides custom formats, tabs, tags, CLI access, and a mature scripting API. Maccy combines native minimalism, OCR search, source-app metadata, and local privacy. PastePal covers queue, collections, OCR/transforms, and screen-sharing concealment. PasteBar provides boards, forms, more than 30 paste operations, protected collections, and backup/restore.
- Raycast now advertises Clipboard History on Windows as well as macOS. Its current feature set includes preserved rich formats, OCR, grouped/sequential paste, encrypted local storage, and broad launcher integration.
- New entrants already market the same words vbuff used as a wedge: local-first, E2EE, OCR, cross-platform, PII masking, AI actions, and secure sync.

**Conclusion:** OCR, generic AI actions, MCP availability, encryption, sync, snippets, pinboards, and cross-platform packaging are useful, but none is a defensible headline alone. A feature-count race would leave vbuff late, broad, and hard to trust.

## Twenty-role panel

| # | Role | Strongest finding | Consequence for vbuff |
|---:|---|---|---|
| 1 | macOS competitor analyst | Apple now supplies basic history; Paste owns breadth and polish; Maccy owns minimalism; PastePal owns a strong one-time-purchase queue/collection niche | Do not launch as "clipboard history for Mac" |
| 2 | Windows competitor analyst | Win+V owns zero-install history; PowerToys owns local transformations | Do not lead with history or generic transforms |
| 3 | Linux/Wayland analyst | Clipboard management still depends on compositor protocols; `wlr-data-control` is deprecated in favor of staging `ext-data-control-v1`, with uneven support | Treat compositor truthfulness and visible degradation as product behavior, not an implementation footnote |
| 4 | Cross-platform analyst | CrossPaste already covers major desktop OSes, Android, rich formats, OCR, MCP, and browser handoff | "Same app everywhere" is necessary but no longer sufficient |
| 5 | Security/privacy analyst | Plaintext SQLite and incomplete native evidence make broad trust language premature | Make privacy claims artifact- and capability-based |
| 6 | Retrieval analyst | The popup searches only the latest 1,000 loaded clips even though paged FTS foundations exist | Wire full-history retrieval before adding more search modes |
| 7 | Product-wedge strategist | A short-lived secret lane and compatibility receipts are sharper than generic history, but both need native proof | Test a narrow technical-work cohort instead of building for everyone |
| 8 | Format-fidelity analyst | The flat `Vec<Flavor>` model loses item topology and native format identity; the current corpus compares fixtures with clones | Fix the data contract, then build a real app-pair lab |
| 9 | Sync analyst | Crypto primitives lack a live authenticated device/transport lifecycle; the plan starts replication before directed handoff | Freeze ambient sync and prove one explicit TTL-bound transfer first |
| 10 | egui interaction designer | Painting is virtualized, retrieval is not; keyboard actions are fragmented across surfaces | Use one intent registry and a paged projection |
| 11 | Accessibility/i18n critic | AccessKit roles and pixel snapshots do not prove screen-reader, focus-trap, BiDi, or text-scale behavior | Treat native AT evidence as a release gate, not a badge |
| 12 | Performance/reliability analyst | Current zero-loss and scale tests exercise pure models or 1,000 rows, not the resident product path | Benchmark real clipboard edges, bounded memory, startup, and end-to-end retrieval |
| 13 | SOLID/DRY critic | Root platform imports, duplicated intents, a GUI monolith, and disconnected contract crates increase change cost | Extract only boundaries required by the first native vertical and freeze speculative APIs |
| 14 | Evidence critic | No current test proves the full headline; fidelity is circular and delivery receipts are caller-supplied | Publish reproducible native, app-pair, residue, delivery, search, and AT evidence |
| 15 | Integration critic | IPC, plugin, browser, editor, terminal, and MCP authorization models can drift before a live dispatcher exists | Keep one scoped grant model; build at most one cooperating editor adapter after the core |
| 16 | Onboarding/resident critic | Background startup can fail before any recovery UI, and hotkey/autostart state is write-only | Make bootstrap, rebind, and observed resident health repairable in-product |
| 17 | Adversarial product critic | 48k Rust lines and seven implementation batches have not produced a switch-worthy native loop or demand evidence | Stop contract accumulation; require activation, retention, and willingness-to-pay gates |
| 18 | Native sequencing critic | Wayland-first retires uncertainty but delays the strongest product evidence | Use a narrow Windows 11 evidence vertical; capability-gate Wayland later |
| 19 | Feasibility/value arbiter | DB-backed recall is the fastest direct value; native capture and delivery remain mandatory | Ship retrieval first, then one native vertical; count prerequisites as cost, not differentiation |
| 20 | Moat/sequencing arbiter | UI controls are copyable; compatibility history, app adapters, and labeled recall evidence compound | Sequence Windows evidence, the fidelity lab, then contextual recall |

## Chosen market position

### Initial user

Start with **developers, DevOps/security engineers, and technical support staff who work across at least two desktop operating systems and regularly move data among browser, terminal, IDE, remote sessions, ticketing tools, and AI assistants**.

This segment has a sharper problem than the general consumer:

- exact whitespace, rich formats, paths, tables, and custom MIME payloads matter;
- a wrong-target paste can be destructive;
- copied API keys, OTPs, credentials, logs, and customer data require different retention and sharing rules;
- source and task context are often easier to remember than the copied text;
- macOS-only or Windows-only tools break the workflow;
- AI tools make broad clipboard access materially riskier.

### Promise

**The evidence-backed clipboard for technical work: recall the right item by context, preserve a tested representation, and apply explicit policy to every vbuff-controlled capture, storage, and disclosure boundary.**

The shorter product line is: **Right clip. Tested format. Explicit evidence.**

This does not promise destination-only secrecy after bytes enter a global OS clipboard, or that an arbitrary target application inserted them successfully. vbuff can prove only the stages it observes: capture evidence, durable storage, clipboard staging, target identity, injection dispatch, and acknowledgements from cooperating integrations. It must label each level independently and never imply that an OS-history marker controls third-party clipboard monitors.

## The four defensible systems

### 1. Open Format Fidelity Lab

Turn the existing format-fidelity corpus into a public product contract:

- capture all simultaneously offered flavors atomically;
- preserve canonical source bytes and derive previews separately;
- test source-app to target-app pairs across OS versions;
- select a destination-compatible representation without mutating the archive;
- publish supported, degraded, and unsupported outcomes;
- let users report only format identifiers, sizes, hashes, capability receipts, and app versions unless they explicitly attach a sanitized sample;
- add adapters as narrow platform modules, not conditions scattered through GUI or core code.

This is both a feature and a compounding asset. Every fixed app pair expands a compatibility corpus and makes regressions visible. Competitors can copy a UI control more easily than a maintained cross-OS evidence matrix.

### 2. Contextual Recall

Build retrieval around how people remember work:

- source application, optional privacy-safe document/project label, time, content type, and neighboring copy sequence;
- session grouping for a browser -> terminal -> IDE workflow;
- query facets such as `app:`, `type:`, `before:`, `after:`, and `session:`;
- "show surrounding copies" from every result;
- local destination-aware ranking based on prior accepted choices, with a visible reason and a deterministic non-learning fallback;
- encrypted context fields, per-app opt-out, and no window-title capture where identity cannot be justified.

The first version must be deterministic. Semantic embeddings may be compared later, but they do not replace source/time/session cues and must never index sensitive items by default.

### 3. Clipboard Policy and Delivery Evidence

Unify capture, storage, sharing, AI, and delivery policy around one typed decision system:

- native concealed/private markers and source rules run before payload persistence;
- sensitive classes route to skip, bounded memory-only, encrypted durable, or explicit one-shot handoff lanes;
- every destination receives a policy decision for allowed flavors, transforms, retention, and disclosure;
- sensitive clipboard writes use an atomic OS-history-excluding operation or fail before bytes are written;
- UI and logs use a receipt ladder: `Staged`, `TargetConfirmed`, `InjectionSent`, and `ApplicationAcknowledged`;
- only a cooperating browser/editor integration may produce `ApplicationAcknowledged`;
- a Trust view explains current capabilities and failures without logging plaintext.

This is stronger than "AES-256" marketing because it defines what data can cross each boundary and supplies test evidence for the decision.

### 4. Scoped Context Gateway for AI

Do not compete by exposing a generic `search_history` MCP tool. Build a least-privilege memory boundary:

- no resources are visible by default;
- grants are per client, collection/session, content class, time range, operation, byte budget, and expiry;
- read, search, add, organize, and transform are separate capabilities;
- secrets and memory-only clips are excluded unless a one-shot user approval explicitly names the item and destination;
- every response carries provenance and is structured as untrusted data, not instructions;
- exact items are previewed before first disclosure to a client;
- grants can be paused, revoked, and inspected without restarting vbuff;
- audits record category, client, scope, count, and outcome, never raw content;
- tool/schema changes require renewed approval.

Paste and CrossPaste already advertise MCP. The opportunity is not MCP availability; it is making clipboard-to-agent access demonstrably narrower and safer than an ambient local server.

## Opportunity scorecard

Scores are directional, not forecasts. `Impact`, `defensibility`, and `fit` use 1-5; effort is `S`, `M`, `L`, or `XL`.

| Candidate | Class | Impact | Defensibility | Fit | Effort | Decision |
|---|---|---:|---:|---:|---|---|
| DB-backed full-history retrieval and summary projection | Direct value | 5 | 4 | 5 | M | **First small slice** |
| Ordered snapshot/item/flavor/native-ID data contract | Release blocker | 5 | 4 | 5 | L | **Before fidelity claims** |
| Native first-OS capture/privacy/hotkey/paste adapters | Release blocker | 5 | 3 | 5 | L | **Now** |
| SQLCipher plus OS-keystore key lifecycle and residue tests | Release blocker | 5 | 3 | 5 | L | **Now** |
| Open Format Fidelity Lab and app-pair corpus | Moat | 5 | 5 | 5 | L | **Now** |
| Atomic sensitive write plus graded delivery receipts | Moat foundation | 5 | 5 | 5 | L | **Now** |
| Contextual recall with source/time/session neighborhood | Differentiator | 5 | 4 | 5 | L | **Next** |
| Import/dry-run migration from Maccy, CopyQ, Ditto, and PasteBar | Acquisition | 4 | 3 | 4 | M | **Next** |
| Clipboard policy rules and Trust evidence UI | Differentiator | 5 | 4 | 5 | L | **Next** |
| Deterministic developer recipes with preview and undo | Differentiator | 4 | 2 | 4 | M | **After context** |
| Explicit E2EE directed handoff with TTL and receipt | Differentiator | 4 | 3 | 5 | L | **Before full sync** |
| Scoped Context Gateway for AI/MCP | Risky extension | 2 | 3 | 3 | XL | **Freeze until a paid native beta** |
| Ambient full-history E2EE sync | Feature | 4 | 2 | 4 | XL | **Later, only after handoff** |
| OCR and screenshot text search | Table stakes | 3 | 1 | 3 | M | **Defer** |
| Semantic search over all history | Commodity/risk | 3 | 1 | 3 | L | **Experiment, off by default** |
| Generic LLM rewrite/summarize/translate palette | Commodity | 2 | 1 | 2 | M | **Skip as a headline** |
| Shared team pinboards | Commodity/operations | 3 | 1 | 2 | XL | **Defer** |
| Mobile peers | Reach | 3 | 2 | 3 | XL | **Defer** |
| General plugin marketplace | Scope risk | 2 | 2 | 2 | XL | **Defer until sandbox exists** |

## Final arbitration: what to implement

1. **Full-history retrieval first.** Replace the 1,000-row in-memory recall path with a paged summary query over the existing store indexes. Keep payload hydration off the egui frame. Gate: 100,000 rows, p95 first results at or below 50 ms, interactive p99 at or below 16 ms after warm-up, and retrieval of items older than the first 1,000.
2. **One verifiable Windows 11 alpha.** Model ordered clipboard items and native format IDs, then implement event-driven capture, privacy/history markers, encrypted storage, target reconfirmation, and honest delivery evidence for a deliberately small app/format matrix. Gate: 14 days as the sole manager, zero silent observed-state loss, zero wrong-target injection, and zero plaintext canaries in DB/WAL/SHM/CAS/temp/log artifacts.
3. **Publish the app-pair Fidelity Lab, then test contextual recall.** Start with roughly 12 browser/IDE/terminal/Office routes. A supported row permits no silent downgrade. After deterministic full-history facets work, require at least a 20% accepted-top-three lift over the text-only baseline across 200 labeled retrievals before treating contextual ranking as a moat.

Freeze ambient sync, MCP, plugin execution, remote automation, and new updater protocol work until the first-OS beta passes both engineering gates and a demand gate: of 20 target users, at least 12 activate without documentation, 8 use vbuff four days per week after 30 days, and 5 are willing to pay at least USD 25. A second native backend, not a shared abstraction, is the prerequisite for any cross-platform parity claim.

## Three critical iterations

### Iteration 1: feature-parity critic

The first pass included cross-platform sync, OCR, AI transforms, MCP, boards, snippets, secure storage, and polished search. Current product evidence invalidated most of this as differentiation. Paste, CrossPaste, PowerToys, CopyQ, Maccy, PasteBar, and newer entrants already cover those boxes in different combinations.

**Change:** remove feature breadth as the strategy. Retain native fidelity, contextual recall, policy, and scoped agent access.

### Iteration 2: feasibility and security critic

The reduced list still assumed vbuff could build sync and AI while its active backend only polls text or image, the live database is unencrypted, and automatic paste is disabled. It also used "verified paste" too broadly even though arbitrary applications do not acknowledge insertion.

**Change:** native proof, encrypted storage, atomic sensitive staging, and graded receipts move ahead of differentiated features. Directed handoff moves ahead of full sync. `ApplicationAcknowledged` is reserved for cooperating integrations.

### Iteration 3: six-month-copy critic

Context filters, a privacy screen, and an MCP permission dialog can all be copied. The surviving question was what compounds after release.

**Change:** make the fidelity corpus, app adapters, policy contract, migration coverage, and accepted source-destination behavior the assets. UI features sit on those systems; they are not the moat by themselves.

## Small implementation slices

Each slice has one owner boundary and can be understood independently.

### Slice 0 - full-history recall

1. Add one `HistoryQuery { query, facets, cursor, limit } -> HistoryPage` boundary returning summaries rather than hydrated bodies.
2. Merge the bounded volatile lane into that projection without persisting it.
3. Run queries off the egui frame and cancel stale generations.
4. Hydrate one selected payload only for preview or delivery.
5. Benchmark the actual popup path at 100,000 rows and record the result in the SLO ledger.

Gate: items older than the first 1,000 are reachable; p95 first results are at or below 50 ms; warm interactive p99 is at or below 16 ms; ten idle repaint cycles trigger zero projection rebuilds.

### Slice A - verifiable Windows core

1. Introduce `ClipboardSnapshot -> ClipboardItem[] -> Flavor[]`, preserving order, native format IDs, realization state, generation evidence, and canonical source bytes.
2. Implement Windows 11 event-driven capture and exact writes for the initial declared formats and app matrix.
3. Wire SQLCipher plus Credential Manager/DPAPI key lifecycle and whole-artifact residue tests.
4. Add atomic history/monitor policy handling, immediate target reconfirmation, and independent delivery evidence states.
5. Keep elevated sessions, RDP, sensitive paste, and every unproven target copy-only or unsupported.

Gate: 10,000 generated Windows clipboard edges are stored exactly once or explicitly gap-accounted; 1,000 round trips per supported format preserve canonical bytes; zero durable secret-canary hits; zero wrong-target injections; and no UI state is stronger than its evidence.

### Slice B - compatibility lab

1. Expand `vbuff-platform/tests/corpus/format-fidelity-v1.json` into versioned source/target fixtures.
2. Add one runner that all native adapters implement.
3. Cover plain text, HTML, RTF, PNG, file lists, URLs, line endings, Unicode, and representative custom formats.
4. Publish a generated matrix from test results.
5. Add a privacy-safe failure bundle.

Gate: every supported fixture round-trips canonical bytes across at least 1,000 cases per supported content class; unsupported pairs degrade visibly and never silently choose a lossy flavor.

### Slice C - contextual recall

1. Add encrypted source application and session metadata to the store boundary.
2. Implement deterministic grouping and query facets in `vbuff-core::recall`.
3. Add a context strip and "surrounding copies" action to the popup.
4. Build a labeled recall benchmark comparing text-only ranking with context ranking.
5. Add local accepted-choice feedback only after deterministic behavior is stable.

Gate: context ranking improves top-5 retrieval by at least 20% on the labeled benchmark without exceeding the search latency SLO. Kill destination learning if it does not improve accepted top-3 choices after 200 labeled retrievals.

### Slice D - safe automation boundary

1. Extract the live IPC dispatcher before enabling external clients.
2. Reuse one capability type across CLI, browser/editor integrations, and MCP.
3. Add expiring virtual views and content-class denial.
4. Run prompt-injection, scope-escalation, replay, and plaintext-log tests.
5. Enable read-only MCP for one explicit collection before any write tool.

Gate: default connection exposes zero clips; cross-scope and secret exfiltration tests stay at zero; every disclosure is attributable to a current grant.

### Slice E - directed handoff

1. Pair two devices with authenticated keys.
2. Send an explicitly selected item or stack, not ambient history.
3. Apply TTL, content-class policy, target device policy, retry, revocation, and receipt.
4. Keep relays blind to payload plaintext and avoid content-derived billing/telemetry.
5. Add durable sync only after handoff recovery and policy behavior are proven.

Gate: two OSes pass interruption, replay, wrong-device, revocation, and expiry tests; relay captures reveal no payload plaintext.

## Product design consequences

- The popup remains the first screen. Do not add a dashboard.
- Search results show source, time, session neighborhood, content type, and policy state without turning rows into cards.
- The primary action label follows the receipt capability: Copy, Stage and Paste, or Send. Never display "Pasted" after only dispatching a shortcut.
- Exact-format choice appears as a compact menu or command palette with a short reason, not a permanent toolbar.
- Sensitive content is masked; a deliberate peek never changes retention or disclosure permission.
- The Trust view explains unavailable native proof and current grants. It is an operational surface, not a marketing score.
- AI suggestions are shown as a compact diff/preview and require a user action. There is no auto-apply mode.
- Platform key labels and permission flows are native to each OS while command names and policy semantics stay shared.

## Explicit anti-features

- No unscoped `search_all_clipboard_history` tool for AI clients.
- No silent LLM transformation or automatic paste rewrite.
- No generic claim that a paste succeeded without an application acknowledgement.
- No background full-history sync before explicit handoff is safe.
- No OCR or embeddings for sensitive and memory-only lanes by default.
- No plugin execution before a real OS sandbox and host-side capability enforcement exist.
- No new 500-item backlog until one strategy slice passes user and release gates.

## Documentation corrections required

The following older claims must not be repeated as current facts:

- "macOS has no built-in clipboard history" is false for macOS Tahoe 26 and later.
- Paste is not accurately described as subscription-only; its site now presents monthly, annual, and lifetime choices.
- Paste now advertises Teams, shared pinboards, OCR/Power Search, Apple Intelligence, screen-sharing privacy controls, and local MCP.
- CrossPaste is no longer accurately summarized as only basic LAN sync; it advertises local OCR, MCP, a Chrome extension, and CLI/plugin roadmap work in addition to LAN-only E2EE.
- Maccy now uses OCR for image search; OCR-derived search text is stored in its local database according to its maintainer.
- Raycast Clipboard History is no longer macOS-only; its Windows product now advertises the feature. Current documentation also covers rich-format and sequential-paste workflows.
- PastePal should be represented as a current macOS competitor with queue, collections, OCR/transforms, screen-sharing concealment, iCloud, and a one-time Pro purchase.
- Pastebot documents a default history of 200 items, not an approximate fixed 500-item cap; the limit is configurable.
- `wlr-data-control` is deprecated by the protocol documentation; new implementation planning should evaluate staging `ext-data-control-v1` and report compositor support rather than promise generic Wayland parity.
- "Every competitor wins at most two corners" is an untestable and now visibly stale absolute. Use a concrete capability matrix and dated evidence instead.
- A universal "zero-loss" claim is not supportable for a polling API. The measurable promise is zero silent loss among observed states plus explicit accounting for detected sequence gaps.

## Primary sources

- [Apple: Clipboard history in macOS Tahoe Spotlight](https://support.apple.com/en-asia/guide/mac-help/mchl40d5b86b/26/mac/26)
- [Microsoft: Windows clipboard history limits and sync](https://support.microsoft.com/en-au/windows/using-the-clipboard-30375039-ce71-9fe4-5b30-21b7aab6b13f)
- [Paste product and pricing](https://pasteapp.io/)
- [Paste updates](https://pasteapp.io/updates)
- [Paste MCP](https://pasteapp.io/mcp)
- [CrossPaste product](https://crosspaste.com/en/)
- [CrossPaste repository and roadmap](https://github.com/CrossPaste/crosspaste-desktop)
- [PowerToys Advanced Paste](https://learn.microsoft.com/en-us/windows/powertoys/advanced-paste)
- [Maccy product](https://maccy.app/)
- [Maccy OCR/privacy maintainer response](https://github.com/p0deje/Maccy/issues/1335)
- [Raycast Clipboard History](https://www.raycast.com/core-features/clipboard-history)
- [CopyQ documentation](https://copyq.readthedocs.io/en/stable/)
- [CopyQ security documentation](https://copyq.readthedocs.io/en/latest/security.html)
- [PasteBar repository](https://github.com/PasteBar/PasteBarApp)
- [Wayland `ext-data-control-v1`](https://wayland.app/protocols/ext-data-control-v1)
- [XDG Global Shortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- [Woodruff and Alexander: 90-day clipboard and drag-and-drop study](https://eprints.lancs.ac.uk/id/eprint/136474)
- [Stolee, Elbaum, and Rothermel: copy/paste habits](https://digitalcommons.unl.edu/cseconfwork/133/)
- [Context reinstatement and episodic remembering](https://doi.org/10.1016/j.cortex.2017.06.007)
- [MagicCopy cross-app context preprint](https://arxiv.org/abs/2604.04307)
- [Google Smart Paste production study](https://arxiv.org/abs/2510.03843)
- [MCP Security Best Practices](https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices)
- [OWASP MCP Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/MCP_Security_Cheat_Sheet.html)
