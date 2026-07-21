# Quarterly scope pruning review

Reviewed on 2026-07-21 for 2026 Q3. The scheduled workflow opens one tracking issue in January, April, July, and October. The checked-in decision record, not the reminder issue, is authoritative.

## Decision rules

Every considered item receives exactly one disposition:

| Disposition | Meaning |
|---|---|
| **Promote** | It is the next bounded batch and has an owner, acceptance evidence, and no skipped prerequisite. |
| **Keep** | It remains in the canonical backlog at its current order. |
| **Defer** | It stays documented but cannot enter the active milestone until the named gate is met. |
| **Cut** | It conflicts with the product boundary or no longer earns its cost; the reason remains in history. |

Use privacy and zero-loss correctness first, then native reliability, accessibility, maintainability, and everyday utility. Repository popularity and novelty are evidence inputs, never automatic promotion. No review may silently expand the canonical 1-600 objective.

## 2026 Q3 record

| Scope | Disposition | Decision |
|---|---|---|
| 251-300 | Promote | Finish as one reviewed implementation/foundation batch, preserving native and external gates rather than claiming live sync or integrations. |
| 301-350 | Keep | It is the next sequential batch only after 251-300 is committed, pushed, merged, and green on required CI. |
| 351-600 | Defer | Keep the order and evidence, but do not pull work forward across the 50-item boundary. |
| 601-610 | Defer | Keep as researched candidates outside the active objective; reconsider only through an explicit goal change. |
| Live sync, hosted plugins, broad native adapters | Defer | SQLCipher/keystore, daemon dispatch, native fidelity, and two-device threat-model evidence remain prerequisites. |
| Marketing or telemetry breadth | Cut from current milestone | It does not outrank private, loss-accounted local capture and must not consume the release-critical path. |

## Mechanical cut line

Stop adding breadth and open a scope decision when any condition is true:

- a prior 50-item batch is not merged and green;
- a critical limitation has no owner or exit evidence;
- SQLCipher, OS-keystore, zero-loss, wrong-target paste, or native privacy-hint work is displaced by convenience work;
- the active workspace grows beyond the nine-crate architecture without an approved ownership split;
- one milestone remains open more than 42 days;
- measured search, idle CPU, startup, memory, or capture-loss status is `Unknown` at a release gate.

## Review template

Record the date, quarter, reviewers, active milestone, last green release/commit, open critical limitations, SLO evidence, dependency/security changes, and the disposition of every range or proposal considered. End with the next single batch, its owner, acceptance commands, explicitly deferred work, and the date of the next review.
