# Implementation batch 351-400

Reviewed at the runtime/foundation/adapted/native-required level on 2026-07-28.
This ledger is the execution overlay for backlog items 351-400 in
[ideas-301-400.md](ideas-301-400.md). Acceptance means the recorded local
checks pass; it does not activate native desktop registration, team transport,
daemon/CLI listeners, plugin execution, hosted voting, or an LTS channel.

## Current runtime boundary

- Desktop shortcut, access-mode, portable/managed profile, permission-repair,
  and theme types are pure policy. Native registration, machine policy loading,
  OS settings navigation, and live theme events are not connected.
- Team roles, approvals, leases, revocation records, denylist, variables,
  metadata audit, fork/comment/broadcast/import/plugin/changelog/simulation
  contracts have no membership store, network transport, encryption, or UI.
- The local RPC schema, completions, headless plans, replay cursor, loopback
  endpoint, backup/health/fixture records, rate limiter, and dry-run envelope do
  not create a daemon listener or command executor.
- Plugin tests, signed action bundles, fetch authorization, marketplace
  metadata, and panic supervision validate bounded evidence only. No plugin
  process or socket is started and no OS sandbox is claimed.
- Release governance is a checked-in policy. Hosted voting and LTS are inactive;
  dogfood evidence exists only after an interval runs, and historical competitor
  research is not current compatibility proof.

## Status vocabulary

| Status | Meaning |
|---|---|
| **Runtime** | Exercised by the current resident binary, popup, store, CLI, or repository automation. |
| **Foundation** | Implemented and tested as a bounded reusable contract, but not connected to the final user-facing surface. |
| **Adapted** | A narrower implementation preserves privacy, portability, or truthful capability reporting. |
| **Native required** | Completion depends on real per-OS APIs, clients, transports, hosted services, credentials, staffing, or assistive-technology evidence. |
| **Rejected** | The proposed mechanism conflicts with a safety or correctness constraint; its replacement is recorded. |

## Item ledger

| Item | Status | Landed evidence | Remaining product work |
|---:|---|---|---|
| 351 | Foundation | [`resolve_hotkey_conflict`](../crates/vbuff-platform/src/desktop_policy/hotkey.rs) returns a deterministic, bounded candidate set only after the requested shortcut is reported unavailable and never labels an alternative registered. | Connect native registration errors, first-run selection UI, persistence, and per-OS conflict evidence. |
| 352 | Foundation | [`LayoutAwareAccelerator`](../crates/vbuff-platform/src/desktop_policy/hotkey.rs) separates a stable physical key id from the current-layout display label and validates both. | Populate labels from native keyboard-layout events and migrate existing logical shortcuts without changing their physical meaning. |
| 353 | Foundation | [`linux_environment_note`](../crates/vbuff-platform/src/desktop_policy/linux.rs) owns concise, capability-honest GNOME, KDE, Sway, Hyprland, X11, and unknown-Wayland guidance. | Select notes from proven compositor/session detection, localize them, and validate instructions on real desktops. |
| 354 | Foundation | [`ResidentAccessMode::HotkeyOnly`](../crates/vbuff-platform/src/desktop_policy/access.rs) models trayless operation while preserving a hotkey recovery surface. | Persist the mode and make resident startup omit tray/menu registration without losing popup recovery. |
| 355 | Foundation | [`ResidentAccessMode::MenuOnly`](../crates/vbuff-platform/src/desktop_policy/access.rs) disables hotkey use while keeping a menu surface. | Wire configuration, unregister an existing hotkey safely, and prove native menu availability before activation. |
| 356 | Foundation | [`ProfileLocation::portable_beside`](../crates/vbuff-platform/src/desktop_policy/profile.rs) derives a bounded profile beside an absolute executable path. | Add an explicit launch/config switch, owner-only permissions, symlink/removable-media handling, migration, backup, and UI disclosure. |
| 357 | Foundation | [`ManagedInstallPolicy`](../crates/vbuff-platform/src/desktop_policy/profile.rs) centrally resolves forced access mode, portable-profile allowance, and a locked hotkey. | Define signed/admin-owned machine policy files, OS locations, schema migration, precedence, diagnostics, and tamper tests. |
| 358 | Foundation | [`permission_repair_plan`](../crates/vbuff-platform/src/desktop_policy/permission.rs) maps typed macOS accessibility, Windows hotkey, and Linux portal failures to one settings locator or bounded repair text. | Add a doctor command, native settings opener, live reprobe, localization, and platform-version evidence. |
| 359 | Foundation | Geometry tests cover negative monitor origins, work-area offsets, tiny displays, invalid scale fallback, and mixed placement constraints. | Run the popup on physical notch/dock/taskbar and mixed-DPI displays and retain screenshot/coordinate evidence. |
| 360 | Foundation | [`NativeThemeState`](../crates/vbuff-platform/src/desktop_policy/theme.rs) changes revision only when light/dark/high-contrast state changes. | Feed native OS theme events into egui, preserve user override semantics, and run live contrast/a11y tests. |
| 361 | Foundation | [`ReadReceiptLedger`](../crates/vbuff-sync/src/team/privacy.rs) is opt-in, bounded, and stores a domain-separated member/item receipt key rather than clip content. | Add authenticated membership, durable encrypted state, retention/reset controls, transport, and team UI. |
| 362 | Foundation | [`SnippetApprovalWorkflow`](../crates/vbuff-sync/src/team/approval.rs) enforces draft/approved/published transitions and a distinct reviewer. | Persist revision history, authorize remote actors, resolve concurrent edits, and build review/publish UI. |
| 363 | Foundation | [`TeamRole`](../crates/vbuff-sync/src/team/approval.rs) centralizes edit, comment, and policy permissions for owner/editor/commenter/viewer. | Bind roles cryptographically to collection membership and enforce them at every storage and transport mutation. |
| 364 | Foundation | [`SharedClipLease`](../crates/vbuff-sync/src/team/sharing.rs) fails closed when an item id is malformed or its deadline has passed. | Enforce expiry in durable stores, transports, caches, offline clients, and UI under clock skew. |
| 365 | Foundation | [`ExternalShareGrant`](../crates/vbuff-sync/src/team/sharing.rs) validates expiry, records revocation, exposes only an item hash, and redacts Debug output. | Design E2EE link keys, server-blind transport, cache invalidation, abuse controls, and verifiable revocation before creating links. |
| 366 | Foundation | [`TeamDefaultDenylist`](../crates/vbuff-sync/src/team/privacy.rs) carries bounded source-app and detector ids only and never employee clip values. | Sign organization policy, define precedence and local enforcement, and prove admins cannot query matched content. |
| 367 | Foundation | [`SharedVariableCatalog`](../crates/vbuff-sync/src/team/privacy.rs) validates bounded names/values and redacts values from Debug. | Encrypt/persist catalog values, authorize resolution, handle secret variables separately, and add preview/edit UI. |
| 368 | Foundation | [`TeamConfigAuditSnapshot`](../crates/vbuff-sync/src/team/privacy.rs) validates nonzero member/policy hashes and a policy revision, then reports health counters without payload fields. | Define collection ownership, freshness, signatures, transport, retention, and an admin view that cannot drill into clips. |
| 369 | Foundation | [`CollectionForkPlan`](../crates/vbuff-sync/src/team/sharing.rs) requires distinct source/private ids and a bounded item count. | Copy encrypted content transactionally, preserve provenance, strip team-only permissions, and expose conflict/space feedback. |
| 370 | Foundation | [`ConflictComment`](../crates/vbuff-sync/src/team/sharing.rs) is role-gated, bounded, and redacts author/body in Debug. | Persist threaded comments, moderate/resolve them, authenticate actors, and build accessible conflict UI. |
| 371 | Foundation | [`EmergencyBroadcast`](../crates/vbuff-sync/src/team/sharing.rs) requires owner authority, revision, deadline, and redacts the message in Debug. | Add signed fan-out, receipt/expiry behavior, offline recovery, rate limits, and high-priority UI without bypassing privacy policy. |
| 372 | Foundation | [`validate_team_import`](../crates/vbuff-sync/src/team/import.rs) bounds batches and per-item scopes and rejects duplicate ids, malformed templates, missing variables, invalid allowlists, and unsafe actions. | Validate assets/signatures/schema, stage imports transactionally, and add a payload-safe review/approval surface. |
| 373 | Foundation | [`ScopedTeamPluginApproval`](../crates/vbuff-sync/src/team/import.rs) checks the active team and manifest hash together with explicit collection and capability sets on every authorization. | Connect publisher trust, plugin sandbox enforcement, revocation, membership policy, and audit UI. |
| 374 | Foundation | [`CollectionChangelog`](../crates/vbuff-sync/src/team/audit.rs) accepts bounded, contiguous metadata-only changes. | Persist signed actor/time records, handle retention and compaction, and render user-readable diffs without payload leakage. |
| 375 | Foundation | [`simulate_team_policy`](../crates/vbuff-sync/src/team/policy.rs) rejects non-synthetic cases and returns reason ids rather than content. | Add an admin editor, representative synthetic corpus, versioned rollout comparison, and false-blocking review before policy activation. |
| 376 | Foundation | [`RpcEnvelope`](../crates/vbuff-ipc/src/operations/rpc.rs) requires an exact schema version and bounded request id; a golden JSON test freezes the current shape. | Activate only after M7 with mandatory handshake, old/new client fixtures, frame limits, authenticated transport, and deprecation rules. |
| 377 | Foundation | [`ShellCompletionCatalog`](../crates/vbuff-ipc/src/operations/completion.rs) provides sorted bounded tag/collection/kind/device metadata completions. | Build the deferred CLI, shell adapters, live metadata query, escaping, and privacy review for completion history. |
| 378 | Foundation | [`HeadlessOperationPlan`](../crates/vbuff-ipc/src/operations/runtime.rs) forbids GUI/tray launch and requires dry-run for mutating headless work. | Add the M7 daemon and M8 CLI executor, explicit apply transition, progress/cancel semantics, and terminal exit contracts. |
| 379 | Foundation | [`EventReplayCursor`](../crates/vbuff-ipc/src/operations/runtime.rs) validates stream identity and a strictly ordered retained window, bounds replay, and reports expiry explicitly. | Persist cursor/stream epochs, authenticate clients, define restart retention, and test disconnect/reconnect against a live daemon. |
| 380 | Foundation | [`PluginTestCase`](../crates/vbuff-plugin/src/governance/test_harness.rs) evaluates hash-only fixtures, output, capability attempts, timeout, and panic evidence deterministically. | Ship a sandboxed runner, author tooling, sanitized fixture builder, resource accounting, and cross-platform golden execution. |
| 381 | Foundation | [`SignedActionBundle`](../crates/vbuff-plugin/src/governance/action_bundle.rs) signs a domain-separated canonical bundle hash whose actions exactly validate against one manifest revision. | Define publisher custody, trust/revocation/rotation, installation transaction, update behavior, and user permission review. |
| 382 | Foundation | [`NetworkFetchPolicy`](../crates/vbuff-plugin/src/governance/network.rs) permits credential-free HTTPS to an exact nonlocal per-action host, returns only host/port/byte-limit authorization, and rejects private/special DNS answers. | Enforce every resolution and redirect outside the plugin process with egress sandboxing, timeout/concurrency limits, and traffic evidence. |
| 383 | Foundation | [`LoopbackWebhookEndpoint`](../crates/vbuff-ipc/src/operations/runtime.rs) rejects non-loopback binds, zero ports, and tokens outside the exact webhook scope. | Implement a listener with token rotation, body/rate/concurrency limits, browser-origin defenses, shutdown, and IPv4/IPv6 tests. |
| 384 | Foundation | [`BackupCommandPlan`](../crates/vbuff-ipc/src/operations/diagnostics.rs) requires encryption, a manifest, and post-write verification. | Implement durable write/close, key handling, scheduled CLI execution, independent restore verification, and user-visible receipts. |
| 385 | Foundation | [`MachineHealthSnapshot`](../crates/vbuff-ipc/src/operations/diagnostics.rs) emits versioned bounded JSON for capture/store/sync counters without clip fields. | Expose it only through authenticated local IPC, freeze compatibility fixtures, and connect truthful live measurements. |
| 386 | Foundation | [`SanitizedFixtureManifest`](../crates/vbuff-ipc/src/operations/diagnostics.rs) requires content removal, metadata review, bounded count, schema, and hash. | Build explicit selection/export, format-specific sanitizers, canary residue scans, and a user review step. |
| 387 | Foundation | [`MarketplaceMetadata`](../crates/vbuff-plugin/src/governance/marketplace.rs) validates categories, examples, exact manifest permissions, protocol range, license, and safe HTTPS documentation. | Add publisher verification, review policy, signed index, revocation, compatibility CI, installation UI, and abuse response. |
| 388 | Foundation | [`PluginSupervisor`](../crates/vbuff-plugin/src/governance/supervisor.rs) disables only the recorded plugin after a panic and retains bounded content-free failure reports. | Put each plugin in a real process/OS sandbox, catch exit/resource failures, restart safely, and keep capture/store independent under hostile load. |
| 389 | Foundation | [`TokenRateLimiter`](../crates/vbuff-ipc/src/operations/rate_limit.rs) applies separate bounded read/write/paste quotas per token hash and server-derived window and rejects global clock rewind. | Centralize it in the future daemon, persist/rotate policy safely, handle concurrent clients, and expose retry metadata without identity leakage. |
| 390 | Foundation | [`MutationRequest`](../crates/vbuff-ipc/src/operations/rpc.rs) and [`MutationPreview`](../crates/vbuff-ipc/src/operations/rpc.rs) give every future mutating contract an explicit DryRun/Apply vocabulary. | Route every M7/M8 mutation through one validation plan and prove dry-run/apply equivalence plus zero side effects before activation. |
| 391 | Native required | [Release governance](release-governance.md) specifies opaque short-lived tokens, aggregate-only output, minimum cohort, retention, replay/rate limits, and an inactive fallback to public issues. | Build and independently review a hosted service before accepting votes; do not add a stable device or account identity. |
| 392 | Foundation | [Compatibility scorecard rules](release-governance.md) require dated product/OS/app versions, reproducible evidence, stale handling, and explicit Pass/Partial/Fail/Unknown. | Publish and continuously rerun the actual import/fidelity/privacy/platform matrix; historical research alone does not populate passing cells. |
| 393 | Foundation | The [dogfood diary template](release-governance.md) records build, interval, coarse workflow counts, reliability/privacy counters, friction, and decision while forbidding content/source/query identifiers. | Complete and publish a real 14-day native interval before treating the diary as release evidence. |
| 394 | Foundation | [`FeedbackEnvironment::redacted_preview`](../crates/vbuff-core/src/feedback.rs) emits bounded content-free diagnostics and opens only an explicit issue draft; governance adds review-before-submit and empty-by-default attachments. | Build the multi-step review UI and test optional attachment sanitizers; never auto-attach logs, screenshots, databases, or exports. |
| 395 | Foundation | [Release governance](release-governance.md) defines pre-1.0 per-release disclosure plus conditional eight-week stable and support-window targets. | Activate a calendar only after signed release rehearsal, two custodians, recovery evidence, and an explicitly supported release. |
| 396 | Native required | The same policy defines an at-most-annual, 18-month target LTS line with observation, migration, rollback/export, native, supply-chain, and recovery gates. | Fund and staff backports, promote a proven signed stable build, and publish supported versions before claiming LTS. |
| 397 | Foundation | [`SECURITY.md`](../SECURITY.md) documents private reporting, severity, acknowledgement/assessment/patch targets, CVE/advisory handling, embargo, notification, and reporter credit; GitHub Private Vulnerability Reporting is enabled for the repository. | Assign at least two responders and drill advisory, CVE, backport, notification, and credential-compromise paths before release. |
| 398 | Adapted | The [portability promise](release-governance.md) keeps versioned offline local export free of account, entitlement, telemetry consent, and network dependencies while explicitly denying a complete current backup claim. | Deliver a user-facing encrypted export/import and clean-machine restore fixtures for each supported release. |
| 399 | Runtime | [`quarterly-scope-review.yml`](../.github/workflows/quarterly-scope-review.yml) opens one quarterly review and [scope-review.md](scope-review.md) maps Promote/Keep/Defer/Cut to Now/Next/Later/Never. | Complete each dated review with current green commit, gates, evidence, owner, and next one bounded batch. |
| 400 | Runtime | The scheduled scope-review template and [entropy accounting](scope-review.md) require at least ten merges/prunes/demotions for each 25 additions and keep unbalanced candidates outside the active objective. | Add historical count automation if the canonical backlog resumes growth; retain human review for semantic merges and cuts. |

## Three review passes (iterations)

- [x] **Iteration 1: correctness and privacy.** The 151 platform/sync/IPC/plugin
  tests and targeted strict clippy pass. Review fixes moved rate-window
  calculation inside the limiter and reject clock rewind, redacted generic RPC
  operations and stable team identifiers from Debug, bound team time values,
  attached team plugin approval to a manifest hash, and rejected local/IP,
  non-HTTPS, credentialed, fragmented, or non-443 plugin fetch targets.
- [x] **Iteration 2: product design and architecture.** Desktop, team,
  operation, and plugin governance were split behind stable facades into 26
  production concern modules of 16-204 lines; a documentation contract caps
  each at 220 lines. README reading order, architecture ownership, all 50 ledger links,
  top-level status tables, release/security recovery paths, and docs contracts
  pass. No visible GUI changed, so the existing visual matrix is unaffected and
  no unsupported native/theme/accessibility claim was added.
- [x] **Iteration 3: release evidence.** The full locked/offline workspace and
  no-default matrices, strict workspace clippy, Linux target suite, release
  budgets, deny/audit/vet, fuzz compilation, workflow parsing, documentation
  contracts, and whitespace review pass. Final review additionally bound team
  plugin authorization to the active team/manifest, rejected duplicate imports
  and zero identity hashes, added post-DNS private-address checks, made rate
  windows globally monotonic, rejected false headless mutation labels and
  unordered replay windows, and bound signed actions to the full manifest hash.

No iteration is complete merely because code or a contract exists. A checkbox
may be marked only after the review ran and its findings were corrected or
explicitly accepted with evidence.

## Acceptance checklist

- [x] All three review iterations above have recorded evidence and resolved findings.
- [x] Formatting and strict clippy pass.
- [x] Locked workspace all-feature and no-default-feature test matrices pass.
- [x] Release performance budgets, dependency policy/audit/vetting, fuzz-target compilation, workflow parsing, documentation contracts, and whitespace review pass.
- [x] Top-level architecture, README, recommendation, and plan agree on status and remaining gates.
- [x] `git diff --check` is clean and a final code/privacy review reports no unresolved finding.

The batch may be accepted only at its documented implementation/foundation
level. Native shortcut/menu/theme/permission evidence, portable/managed profile
activation, team transport, daemon/CLI dispatch, loopback listener, verified
backup/restore, plugin process sandbox, hosted voting, LTS staffing, and release
operations remain separate gates.
