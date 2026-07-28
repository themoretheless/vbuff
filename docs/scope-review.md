# Quarterly scope pruning review

Reviewed on 2026-07-28 for 2026 Q3. The scheduled workflow opens one tracking issue in January, April, July, and October. The checked-in decision record, not the reminder issue, is authoritative.

## Decision rules

Every considered item receives exactly one disposition:

| Product lane | Review disposition | Meaning |
|---|---|---|
| **Now** | **Promote** | It is the one active bounded batch and has an owner, acceptance evidence, and no skipped prerequisite. |
| **Next** | **Keep** | It is the next ordered candidate after Now passes every merge and release gate. |
| **Later** | **Defer** | It stays documented but cannot enter the active milestone until the named gate is met. |
| **Never** | **Cut** | It conflicts with the product boundary or no longer earns its cost; the reason remains in history. |

Use privacy and zero-loss correctness first, then native reliability, accessibility, maintainability, and everyday utility. Repository popularity and novelty are evidence inputs, never automatic promotion. No review may silently expand the canonical 1-600 objective.

## 2026 Q3 record

| Scope | Lane / disposition | Decision |
|---|---|---|
| 351-400 | Now / Promote | Finish as one reviewed implementation/foundation batch, preserving desktop, team, daemon, plugin, hosted-service, and release-operation gates. |
| 401-450 | Next / Keep | It is the next sequential batch only after 351-400 is committed, pushed, merged, and green on required CI. |
| 451-600 | Later / Defer | Keep the order and evidence, but do not pull work forward across the 50-item boundary. |
| 601-630 | Later / Defer | Keep as researched candidates outside the active objective; reconsider only through an explicit goal change. |
| Live sync, hosted plugins, team sharing, broad native adapters | Later / Defer | SQLCipher/keystore, daemon dispatch, native fidelity, sandbox, and two-device threat-model evidence remain prerequisites. |
| Marketing or telemetry breadth | Never / Cut from current milestone | It does not outrank private, loss-accounted local capture and must not consume the release-critical path. |

## Entropy accounting

For every 25 ideas added to canonical or candidate lists, the same review must
merge, prune, or demote at least 10 existing ideas. Record the added, merged,
pruned, and demoted counts with links to affected rows. Candidate research may
continue when the ratio is not met, but it cannot expand the active 1-600
objective or enter Now.

## Mechanical cut line

Stop adding breadth and open a scope decision when any condition is true:

- a prior 50-item batch is not merged and green;
- a critical limitation has no owner or exit evidence;
- SQLCipher, OS-keystore, zero-loss, wrong-target paste, or native privacy-hint work is displaced by convenience work;
- the active workspace grows beyond the nine-crate architecture without an approved ownership split;
- one milestone remains open more than 42 days;
- measured search, idle CPU, startup, memory, or capture-loss status is `Unknown` at a release gate.

## Review template

Record the date, quarter, reviewers, active milestone, last green release/commit, open critical limitations, SLO evidence, dependency/security changes, entropy counts, and the Now/Next/Later/Never disposition of every range or proposal considered. End with the next single batch, its owner, acceptance commands, explicitly deferred work, and the date of the next review.
