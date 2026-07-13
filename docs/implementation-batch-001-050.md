# Implementation batch 001-050

Reviewed on 2026-07-14. This ledger is the execution overlay for engineering backlog items 1-50 in [architecture.md](../architecture.md). It records what is usable by the current binary, what exists as a tested library contract, what was deliberately adapted, and what still requires a native clipboard or network transport.

## Status vocabulary

| Status | Meaning |
|---|---|
| **Runtime** | Exercised end to end by the current resident binary and store. |
| **Foundation** | Implemented and tested as a reusable contract or algorithm, but not yet connected to a native backend, transport, or final UI. |
| **Adapted** | The original proposal was narrowed or replaced with a safer implementation. |
| **Native required** | The data contract and gate exist, but truthful completion depends on OS APIs unavailable through the `arboard` fallback. |
| **Rejected** | The proposed mechanism conflicts with a correctness constraint; the replacement is recorded. |

## Item ledger

| Item | Status | Landed evidence | Remaining product work |
|---:|---|---|---|
| 001 | Native required | Structured app, title, document, URL, and selection-rectangle provenance lives in [`vbuff-types`](../crates/vbuff-types/src/lib.rs). | Populate it atomically in the macOS, Windows, X11, and Wayland backends. |
| 002 | Runtime | Hash-and-nonce suppression is bounded in [`capture/ledger.rs`](../crates/vbuff-core/src/capture/ledger.rs), and paste/self-test writes register lineage. | Native backends should also publish the private sentinel flavor; `arboard` uses the hash fallback. |
| 003 | Runtime | [`AdaptivePollScheduler`](../crates/vbuff-core/src/capture/scheduler.rs) tightens after changes and relaxes while stable. | Native keyboard/app-activation hints can make the cadence predictive on macOS. |
| 004 | Foundation | Flavor-growth closure and extension rules are tested in [`capture/coalesce.rs`](../crates/vbuff-core/src/capture/coalesce.rs). | Wire them to event batches once native backends expose multiple notifications per generation. |
| 005 | Native required | Every flavor has origin, realization, and integrity receipts; the gate requires one realized flavor and the popup marks incomplete clips. | Native promised-data and X11 INCR readers must report failed/truncated realization accurately. |
| 006 | Adapted | Capture removes only byte-identical redundant flavors without copying large bodies in [`capture/integrity.rs`](../crates/vbuff-core/src/capture/integrity.rs). | HTML/RTF extraction and image equivalence need audited canonicalizers before semantic pruning is safe. |
| 007 | Runtime | Probable OTPs become masked sensitive, local-only clips with a 90-second hard TTL; expiry refreshes the GUI, uses SQLite secure deletion, and truncates the WAL with an on-disk canary test. | Tune precision/recall with a dedicated OTP corpus and native secret hints; SQLCipher remains mandatory defense while data is live. |
| 008 | Runtime | A byte-free bounded skip ring and 30-second, hash-bound, generation-bound single-use recovery action are wired through popup, diagnostics, and capture. | Native concealed markers are required for broad real-world coverage. |
| 009 | Foundation | Normalized, whitespace, and growing-selection transform relations are tested in [`capture/coalesce.rs`](../crates/vbuff-core/src/capture/coalesce.rs). | Store alternate representations under one logical item and expose the fold decision in UI. |
| 010 | Runtime | Startup performs a write, observe, suppress, and restore self-test while protecting the restored clipboard from recapture. | Invoke the same probe after a future native hook re-subscribe/restart. |
| 011 | Runtime | Ordered app/title/URL predicates and skip/plain-only/strip-image/sensitive actions are evaluated at the single capture gate. | Title and URL matching remains dormant where the fallback backend cannot provide provenance. |
| 012 | Runtime | Generation tracking records exact gaps and stale observations in the persistent capture-loss ledger. | Native backends must supply monotonic generation identities. |
| 013 | Native required | PRIMARY and CLIPBOARD are distinct inputs and the intent gate is tested. | Linux native backends must emit stability and intent observations. |
| 014 | Native required | Per-flavor BLAKE3 receipts are stamped and verified before persistence. | Native readers must re-check owner/generation around the multi-flavor read. |
| 015 | Runtime | SQLite schema v4 maintains prose `unicode61` and code-aware trigram FTS5 indexes. | Broaden code-kind detection and benchmark non-Latin identifier behavior. |
| 016 | Runtime | FTS automerge is tuned and bounded idle maintenance runs optimize plus integrity checks after meaningful churn. | Add segment-count telemetry when SQLite exposes a stable low-cost signal. |
| 017 | Foundation | SimHash, four indexed bands, exact Hamming filtering, and the distance-four full-scan correctness fallback are implemented. | Connect near-duplicate decisions to capture UX instead of silently collapsing rows. |
| 018 | Foundation | Raw-RGBA dHash values and indexed near-image lookup are stored and tested. | Generate canonical thumbnails for encoded formats and add a user-controlled collapse action. |
| 019 | Adapted | A lazy, local, int8 384-dimensional feature vector reranks bounded candidates without model downloads or network access. | A real MiniLM/sqlite-vec backend remains opt-in future work; the current feature hash is not marketed as semantic understanding. |
| 020 | Runtime | Host, color, ISO-date, and language facets are extracted into an indexed side table and parsed from search queries. | Add masked financial facets only after false-positive and privacy review. |
| 021 | Runtime | `(hash, kind)` CAS refcounts are transactional triggers; migration rebuilds them from flavor rows, and duplicate-flavor/cross-kind cases are tested. | Add crash-injection coverage around file installation and transaction commit. |
| 022 | Runtime | CAS paths are kind-separated and two-level sharded, with per-kind spill thresholds and symlink-avoiding GC. | Add a generation tier only if measured directory pressure justifies it. |
| 023 | Runtime | On-disk upgrades create a SQLite backup, checksum manifest, dry-run copy, schema/row verification, transactional live migration, and atomic rollback. | Live-file swap is intentionally deferred because SQLite sidecars and open handles need a platform-tested protocol. |
| 024 | Rejected | FTS keeps a bounded plain projection inside the database. | Stock FTS5 cannot tokenize a zstd-compressed external-content column without a custom tokenizer; adding one would enlarge the trusted parser surface and break `snippet()` semantics. Revisit only with measured size evidence. |
| 025 | Foundation | [`SearchSession`](../crates/vbuff-store/src/lib.rs) owns compiled query text and a pinned/updated/sequence keyset cursor. | Connect session paging and scroll restoration to the popup's store-backed search path. |
| 026 | Adapted | A latency/row-count planner switches query execution from LIKE to FTS automatically. | Both FTS indexes are currently maintained from schema creation; true zero-write-cost deferred backfill needs a migration-safe index state machine. |
| 027 | Runtime | A startup-rebuilt Bloom filter bypasses the dedup SELECT for definite-negative hashes. | Add measured false-positive and rebuild-time telemetry at large history sizes. |
| 028 | Runtime | Idle maintenance recomputes rolling content hashes, repairs safe mismatches, and quarantines uniqueness conflicts into content-free records that never retain flavors or content hashes. | Surface those records in a future doctor UI without exposing clip content in logs. |
| 029 | Foundation | Item presence and tags use add-wins observed-remove sets; concurrent delete plus pin preserves the item. | Persist replicas and exchange operations over the future sync transport. |
| 030 | Foundation | Name, notes, color, and pinned state have independent LWW registers. | Add UI editing and durable sync-log serialization. |
| 031 | Foundation | Content keys are wrapped independently and can be rewrapped to a new root epoch without payload re-encryption. | Integrate with encrypted local storage and scheduled peer rotation. |
| 032 | Foundation | Hybrid logical clocks are monotonic across local clock rollback, clamp hostile remote future time, and tie-break by node. | Persist per-device counters and reject rollback at transport/session boundaries. |
| 033 | Foundation | X25519 plus HKDF plus XChaCha20-Poly1305 sealed envelopes hide sender identity from the recipient envelope. | Authenticate transport sessions and define replay protection. |
| 034 | Foundation | Relay wrappers expose only an epoch-rotating keyed route tag and sealed payload. | Build the relay protocol, abuse controls, and traffic lifecycle. |
| 035 | Foundation | Membership is hash-chained, whole-set SAS-bound, semantically replay-verified, low-order-key checked, and rekeyed after revocation. | Add signed membership distribution and pairing UI before treating it as a trust ceremony. |
| 036 | Foundation | A deny-by-default selective-sync DSL is parsed and evaluated before encryption; sensitive/local-only clips are unconditionally denied. | Add settings validation, policy preview, and durable policy versions. |
| 037 | Foundation | Per-device lanes constrain kinds, collections, and byte size asymmetrically. | Advertise and negotiate lanes over authenticated sessions. |
| 038 | Foundation | Sorted Merkle range roots localize changed and missing records for anti-entropy. | Replace positional reconciliation if insertion-shift benchmarks show unacceptable amplification. |
| 039 | Foundation | Ed25519 wipe receipts bind device, item hash, epoch, and application time. | Collect peer receipts and expose pending/offline devices in UI. |
| 040 | Foundation | A 24-word BIP39 phrase deterministically derives the recovery membership root with zeroized intermediate seed material. | Add guarded display, confirmation, and restore UX. |
| 041 | Foundation | Size-bounded zstd snapshots are encrypted, authenticated, and decompressed through a hard output cap. | Encode the seed into QR/local-handoff frames and prioritize pinned snippets. |
| 042 | Foundation | Local sync events form a signed, hash-chained ledger verified against device keys. | Persist it, rotate files, and build a content-free audit view. |
| 043 | Foundation | HMAC capability tokens are scoped, expiring, revocable, size-bounded, and one-shot; consumed/revoked replay state is pruned at token expiry. | Bind verification to the target session and persist active replay state atomically. |
| 044 | Foundation | Payloads are randomly padded to power-of-two buckets up to 64 MiB before sealed encryption, with checked cross-platform length decoding. | Decide whether cover traffic is worth its battery and bandwidth cost. |
| 045 | Runtime | Durable drop-reason counters and session saved/skipped/lost totals share one vocabulary and are visible in the popup. | Add zero-loss scenario tests against native rapid-copy streams. |
| 046 | Runtime | The adaptive scheduler responds to clipboard changes and miss risk; idle GUI polling was replaced by event-driven hotkey/tray/second-instance wakeups. | Add macOS battery/app-activation signals in the native backend. |
| 047 | Runtime | Capture and maintenance have rolling CPU/wakeup budgets, trip counters, warnings, and automatic interval backoff. | Calibrate thresholds with release-mode multi-day measurements per OS. |
| 048 | Runtime | Structured tracing carries content-free fields through a formatter that redacts sensitive field names and escapes values. | Add a CI log-canary test across every trace level and third-party error path. |
| 049 | Runtime | A fixed-capacity lock-free snapshot ring stores content-free histograms and is atomically dumped by the panic hook. | Abort/signal-safe dumping needs a lower-level preallocated format and remains unresolved. |
| 050 | Runtime | An atomic external heartbeat publishes pid, schema, health, last-capture age, cadence, loss totals, and budget trips every five seconds. | Add `vbuff doctor` and packaging-specific watchdog examples. |

## Three review passes

| Pass | Focus | Corrections made before acceptance |
|---|---|---|
| 1 | Correctness and security | Bound skipped recovery to exact content/generation, fixed stale-generation recovery, rebuilt v4 CAS counts from source rows, made HLC monotonic through logical-counter rollover, replay-validated membership authorization, rejected cross-item CRDT merges, bounded wire/bootstrap/thumbnail decoding, validated RGBA dimensions, masked sensitive rows, and made quarantine content-free. |
| 2 | Performance and concurrency | Replaced 100 ms GUI polling with event-driven hotkey/tray/second-instance wakeups, kept a five-second hidden supervisory repaint and one-second visible refresh, moved metrics history to `ArrayQueue`, removed large-body copies from flavor pruning, bounded similarity reranking, pruned stale thumbnail textures, and added adaptive subsystem backoff. |
| 3 | UX and documentation | Added compact saved/skipped/lost totals, incomplete/sensitive/local-only/expiry metadata, a timed deliberate-recovery action, immediate snapshot refresh after TTL/quarantine maintenance, and explicit current-versus-target documentation. |

## Acceptance gate

The batch is accepted only when `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, the no-default-feature root tests, and `git diff --check` all pass. Native-only and transport-only rows remain visible dependencies for their owning milestones; they are not silently relabeled as shipped product features.
