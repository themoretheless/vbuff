# Release and backlog governance

Reviewed on 2026-07-28. This document turns backlog items 391-400 into
explicit project rules without pretending that a hosted voting service, an
LTS release line, or a public support organization already exists.

## Claim boundary

- The repository is pre-1.0 and has no active LTS channel.
- Release cadence and support windows below are activation rules, not a
  promise that unsigned or unreleased builds receive production support.
- Hosted roadmap voting remains inactive. No account, device identifier,
  email address, or clipboard-derived value may be introduced merely to rank
  feature demand.
- Product comparisons are evidence records. An untested cell is `Unknown`,
  not an inferred pass.

## Account-free roadmap voting contract

A future voting endpoint may activate only after all of these conditions pass:

1. A client obtains one opaque, short-lived voting token without sending an
   account, email address, clipboard metadata, or stable installation id.
2. The service stores the roadmap item, coarse issuance window, and spent-token
   digest only. Raw IP addresses and user-agent strings are excluded from the
   product database and follow the shortest infrastructure retention available.
3. One token can cast or replace one vote during its window. Replay, token
   stuffing, and unbounded minting are rate limited and independently tested.
4. Public output is aggregate counts with a minimum cohort threshold. There is
   no voter list, per-device history, or cross-window profile.
5. The repository publishes the protocol, retention, abuse limits, and a
   disable switch before the first request is accepted.

Until those conditions have deployment and privacy evidence, roadmap demand is
collected through public issues and the quarterly scope review only.

## Compatibility scorecard

The scorecard compares inspectable behavior, not marketing claims.

| Dimension | Pass rule | Evidence | Stale rule |
|---|---|---|---|
| Import | A published fixture imports with an explicit loss report. | Fixture hash, tool/version, command, result | Re-run after either format changes. |
| Representation fidelity | The source/destination app pair preserves each claimed flavor. | Public app-pair receipt and screenshot where safe | Re-run after OS, toolkit, or app major update. |
| Privacy | Capture, storage, export, and network defaults match the documented boundary. | Configuration, traffic/residue test, limitation link | Any boundary change invalidates the row. |
| Platform support | A clean native session passes capture, recall, copy/paste fallback, DPI, theme, and accessibility checks. | OS build, session/compositor, test log | Older than two supported release cycles is `Unknown`. |

Every cell carries the tested product version, OS/application version, evidence
date, and one of `Pass`, `Partial`, `Fail`, or `Unknown`. The current competitor
catalog remains in [competitive-analysis.md](competitive-analysis.md); this
method does not upgrade its historical research into current conformance proof.

## Dogfood diary

Maintainers may publish one content-free record per dogfood interval:

| Field | Required content |
|---|---|
| Build | Commit or signed release identifier |
| Environment | OS/build and compositor or desktop shell |
| Interval | Start/end dates and active days |
| Workflows | Counts by broad action only: capture, search, copy, explicit paste |
| Reliability | Crashes, detected capture gaps, degraded capability time |
| Privacy | Number of policy blocks by reason id; never payload or source identity |
| Friction | Reproducible issue links and the smallest affected workflow |
| Decision | Continue, rollback, or block release, with owner and evidence |

Window titles, source applications, query text, clip hashes, clipboard bytes,
URLs, paths, and account identifiers are forbidden. A diary is release evidence
only after the stated interval has actually run.

## Redacted bug reports

The report flow is review-before-submit:

1. `FeedbackEnvironment::redacted_preview` generates bounded environment and
   capability text with no clip content, source ids, paths, window titles, or
   URLs.
2. The user sees the exact outgoing text and may remove any field.
3. Attachments are opt-in and start empty. Database files, raw logs, crash
   dumps, screenshots, and exports are never attached automatically.
4. The final action opens a draft; vbuff does not submit a report silently.
5. Canary tests cover home paths, email addresses, common token prefixes,
   control characters, and the report size ceiling.

The content-free generator is present today. A multi-step in-app review surface
remains a release-gated foundation.

## Release train and support lines

Before 1.0, every release note states its own support window and no regular
train is implied. The post-1.0 target policy may activate only with two release
custodians and green signing/recovery drills:

| Channel | Target cadence | Target support |
|---|---|---|
| Stable | One minor window every eight weeks; patches as required | Current minor plus the immediately previous minor for 90 days |
| LTS | At most one promoted minor per year | Security and critical data-loss fixes for 18 months |
| Preview | No fixed cadence | No support promise; never auto-promoted |

An LTS candidate must complete a 30-day stable observation window, migrations
from every supported line, clean rollback/export drills, the native evidence
matrix, dependency audit/vet, and credential recovery. Missing maintainers,
signing, native proof, SQLCipher, or restore evidence blocks promotion.

## Security advisory process

[SECURITY.md](../SECURITY.md) owns private reporting, triage, CVE, patch,
notification, and embargo rules. Clipboard disclosure, silent loss, wrong-target
paste, signing compromise, authentication bypass, and export/backup exposure
receive the highest severity until disproved. Public details wait until a fix or
a documented protective action is available.

## Portability promise

- Local data export remains available without an account, subscription,
  telemetry consent, or active network service.
- Every public export has a version, deterministic compatibility note, and
  documented loss behavior.
- A final supported release must retain an offline export path and migration
  instructions.
- The project does not claim that the current library-level JSON envelopes are
  a complete user-facing backup, restore, or SQLite compatibility feature.

## Cutline and entropy budget

The quarterly process in [scope-review.md](scope-review.md) maps each proposal
to `Now`, `Next`, `Later`, or `Never`. Only `Now` may consume the active
milestone. A proposal cannot enter `Now` across an unmet privacy, native,
storage, transport, or release-evidence gate.

For every 25 ideas added to any canonical or candidate backlog, the same review
must merge, prune, or demote at least 10 existing ideas. The review records
added, merged, pruned, and demoted counts plus links to the affected rows. An
unbalanced change may be researched, but it cannot expand the active objective.
