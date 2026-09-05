> Storage update: DuckDB now supplies candidates through one store owner. The SQLite timings below are historical measurements, not DuckDB performance evidence. See [the migration design](duckdb-migration.md).

# Full-history recall: first implementation

Reviewed against the working tree on 2026-09-05.

The native popup now searches every active history row for non-empty queries
and scoped views. Empty, unfiltered history still shows a bounded recent list.
The existing natural-query parser, substring matching, typo tolerance and
sensitive-payload exclusion remain authoritative. The older Store::search
FTS/LIKE API is unchanged: switching to its different grammar would change
results, including Unicode and substring behavior.

## Runtime and bounds

- One sleeping worker with a latest-request mailbox; query changes replace queued
  work and cancel running work between batches. Hiding/clearing search releases
  published results. Results are accepted only for the current query, scope and
  history revision. A store failure is shown as a failure with recent items,
  rather than an authoritative empty history.
- Coarse SQL candidate filtering avoids decoding irrelevant rows. For ASCII
  typo queries, either the first or last non-overlapping fragment survives one
  edit; short queries use single-character fragments. The Rust core verifies
  every candidate. Unicode, embedded NULs and incomplete legacy projections
  take the conservative fallback. Tests compare this path with the full core
  scan across insertion/deletion/substitution cases and legacy/Unicode fixtures.
- Sequence-keyset batches release the store mutex between reads. A correlated
  annotation eligibility check keeps SQLite on the row-sequence traversal,
  avoiding repeated annotation-index scans and sorting for each page.
- At most 64 rows per batch, stopping after 8 MiB of inline bodies. One individual
  clip can exceed that budget. Results retain at most the current GUI memory-policy
  limit (up to 1,000), and at most 32 MiB of bodies except for a single oversized
  best match. The displayed total includes matches beyond the retained results.
- Archived and expired rows never enter a store batch. Memory-only records are
  included without persistence. Expired search results are released by maintenance
  even while the store is busy. New capture/deletion invalidates published results.
- Large external image/file bodies stay as references in UI snapshots. Text is
  still fully hydrated for composition/editing: this is not yet a universal
  metadata-only card model. Paste resolves the original clip by ID.
- Image decoding and external-image retrieval run on one lazy worker. At most two
  requests are pending, and at most 32 textures of 320x320 remain cached (12.5 MiB
  of RGBA texture data, excluding driver overhead and decoding buffers).
- Delete keeps one original for the existing five-second Undo, allowing restoration
  even after external blob collection. This is process-only recovery.
- Hard-expiry cleanup now has a partial expiry index. Every insert already invokes
  this cleanup; the index avoids scanning all non-expiring history on every copy.
  Neither expiry guards nor the data/schema version change.

## Saved searches

The History surface can save, reopen and remove up to 24 named query/scope pairs.
They persist in owner-local configuration, are omitted from shareable exports,
and redact their names and terms in Debug output. Saving is explicit; arbitrary
query history is not recorded. Relative times are interpreted when recall runs.

## Evidence

The release-mode `history_search::tests::full_history_latency_baseline` exercises
production result collection against in-memory SQLite with a rare match in the
oldest row. A single local run measured:

| Rows | Background collection |
|---:|---:|
| 1,000 | <1 ms |
| 10,000 | 2 ms |
| 100,000 | 30 ms |

These are individual fixture timings, not p95 measurements, disk benchmarks,
GUI first-paint latency, idle CPU, or mixed-media soak evidence. The proposed
100 ms p95 target at 100,000 rows is not established by this single-query run. The initial joined batch query
measured about 942 ms at only 10,000 rows; the keyset query removed that growth.

Regression coverage includes old records outside the recent snapshot, typo and
structured-query compatibility, archived rows, sensitive payload exclusion,
volatile metadata recall, newest-request/revision publication, memory limits,
search-only expiry, deferred image bytes, and Undo after blob GC. GUI tests
exercise selecting remote-from-snapshot results, ignoring obsolete results,
and reopening saved scopes. The new full-history view also has a rendered golden;
existing theme, DPI, responsive-layout and keyboard tests remain applicable.

## Remaining work in the first stage

SQL candidate filtering still scans the text projection; broad, Unicode and
legacy queries can require full core scanning. An indexed candidate path and
representative p95 measurements remain follow-up work. Recent-list mutations still reload their
bounded text snapshot. Fully lazy text cards and incremental list updates must
first move editing/Undo/compose actions to resolve originals explicitly.
Measure disk-backed mixed histories and GUI latency before declaring the complete
"instant history" milestone finished. Working sets, parameterized snippets,
transform previews, privacy-pause extensions and native storage/capture work
remain subsequent product steps.

## Keyboard layouts

Free-text recall accepts English QWERTY / Russian ЙЦУКЕН keyboard-layout variants in both directions (`ghbdtn` → `привет`, `руддщ` → `hello`). Original matching runs first; a layout fallback has a lower score and uses the same sensitive-content exclusions. The tag catalog, `tag:` values and `app:` values also accept layout variants. Structured operator names are not rewritten, and this is not phonetic transliteration (`privet`). Stored text and the visible query remain unchanged.

Both database candidate selectors retain non-ASCII rows for ASCII queries and scan without the ASCII prefilter for Cyrillic queries. This keeps layout matches available to the shared matcher, including records beyond the recent UI snapshot.
