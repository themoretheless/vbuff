# Decision gates for batch 151-200

Reviewed on 2026-07-18. This record turns backlog items 185-197 into stop/go rules. A gate is not complete because a contract exists: the named evidence must be collected by the owner role, and missing evidence remains `Unknown` or external work rather than an inferred pass.

## Registered gates

| Backlog | Gate | Owner role | Pass / continue rule | Kill, cut, or re-scope rule | Required evidence |
|---:|---|---|---|---|---|
| 185 | Popup polish spike | Product design lead | At least 10 blinded comparisons; no RTL shaping failure; at most 40% prefer the documented alternative. | More than 40% prefer the alternative, or any reviewed RTL fixture mis-shapes: use the documented retained-UI fallback. | Participant scorecard, fixture set, and theme/DPI/RTL screenshots. |
| 185 | Zero-knowledge relay spike | Sync security owner | A packet capture proves payload and embedding artifacts are ciphertext-only within 14 elapsed days. | Day 14 without the proof: cut internet relay scope to LAN-only and keep relay work out of the release. | Packet capture, key/transcript description, and a ciphertext-content scan. |
| 186 | Backend ordering | Platform lead | Retire Wayland uncertainty before adding another easy backend: probe GNOME, KDE, and sway, then record the support decision. | Do not expand the native backend matrix while the Wayland decision is incomplete. | Three machine reports produced by `scripts/wayland-reality-check.sh` plus manual copy/hotkey/paste observations. |
| 187 | Tracer bullet | Runtime lead | The already-landed resident `arboard` loop remains the integration tracer and proves copy -> store -> popup -> paste. | Do not delete working production-path code merely to recreate a throwaway demo after the fact; revisit only if the loop is replaced wholesale. | Existing root integration tests and popup snapshots. |
| 188 | Format oracle | Platform lead | Every backend maps and round-trips the versioned corpus byte-identically. | A missing mapping or degraded unapproved flavor blocks that backend. | `format-fidelity-v1.json` and `format_fidelity_corpus.rs`. |
| 189 | Early packaging | Release lead | The first-OS artifact installs and runs doctor/verification before the backend fans out. | A package that only works from a Rust checkout does not pass M4. | `packaging-smoke.yml`; final signed clean-VM packages remain release infrastructure work. |
| 190 | Wayland reality check | Linux platform owner | One explicit `full`, `capture_on_summon`, or `unsupported` decision per GNOME, KDE, and sway environment. | Missing real-session evidence is `ProbeIncomplete`; no compositor receives a first-class claim from capability-model tests alone. | Script JSON plus compositor/version, portal, capture, and focused-paste notes. |
| 191 | Sync sub-spikes | Sync lead | Start each spike only after the preceding spike passes. A later kill does not erase evidence from an earlier pass. | A killed spike prevents dependent spikes from starting and narrows the release claim at that boundary. | Four independently reviewed spike reports. |
| 192 | Scope tripwires | Maintainer | At most 9 workspace crates in the current overlay, at most 1 added MVP milestone, and no open milestone beyond 42 days. | Crossing any threshold forces a cut-line review before new scope is accepted. | `tests/scope_contract.rs`, Cargo metadata, and milestone dates. |
| 193 | First-OS dogfood | Runtime owner | vbuff is the only clipboard manager for at least 14 days with zero silent-loss incidents and zero wrong-target pastes. | Any silent loss or wrong-target paste resets the evidence window and blocks backend fan-out. | Daily content-free incident ledger. This remains external human evidence. |
| 195 | M5 ordering | Privacy owner | Capture gate, AI gate, capture health, and default-deny behavior land before optional recipes or snippets. | Convenience work is cut first whenever the privacy floor is incomplete. | Gate tests, Trust evidence, then transform tests in that order. |
| 196 | Data contract freeze | Store/IPC owners | Schema, hash vector, format keys, and hello wire JSON match the versioned v1 fixtures. | Any intentional change requires a new contract version, migration/compatibility note, and old-reader behavior; silent drift blocks merge. | `tests/data_contract_freeze_v1.rs` and `docs/data-contract-v1.md`. |
| 197 | Per-milestone SLO | Performance owner | Each resident milestone records zero loss, search p99, idle CPU, and login-ready samples against the immutable budget ledger. | Missing data is `Unknown` and blocks release; breach blocks the milestone until fixed or explicitly re-scoped. | `MilestoneSloLedger`, PR-triggered `performance.yml`, and host-normalized runtime measurements. |

`crates/vbuff-core/src/delivery.rs` and `crates/vbuff-core/src/slo.rs` encode the deterministic parts of these rules. Real compositor, dogfood, signing, and packet-capture evidence cannot be manufactured by a unit test and stays explicitly external.

## Sync spike order

| Order | Spike | Exit evidence | Independent cut line |
|---:|---|---|---|
| 1 | Discovery and SAS/QR pairing | Two fresh identities discover, pair, and agree on authenticated peer identity. | Keep sync disabled if pairing is ambiguous or replayable. |
| 2 | Authenticated transport | Noise/TLS-equivalent session plus ciphertext-only packet capture. | Stop at paired devices with no replication if transport proof fails. |
| 3 | Plain-text replication | Deterministic convergence and reconnect replay for bounded text clips. | Ship no typed/large-object claim if convergence is not proven. |
| 4 | Typed CAS replication | Multi-flavor and large-object transfer preserves hashes, bounds, and policy. | Keep sync text-only if CAS transfer cannot meet fidelity and resource budgets. |

## Build-versus-buy fallback ladder

| Component | Default move | Escalation trigger and next rung | Hard stop / honest degradation |
|---|---|---|---|
| `tray-icon` | Use behind the narrow `src/tray.rs` adapter. | Wrap a verified platform quirk; fork only for an upstream-blocking regression with a bounded patch. | Keep hotkey and direct launch usable if the tray is unavailable. |
| `global-hotkey` | Use on macOS, Windows, and supported X11 sessions behind the event-loop owner. | On Wayland, use the GlobalShortcuts portal rather than teaching the generic crate compositor policy. | If the portal is absent, expose manual/tray summon; do not claim a global shortcut. |
| OS keyring | Add only behind a small key-provider interface when SQLCipher lands. | Wrap the platform credential store; a user-supplied unlock secret is the reviewed fallback. | Never place the database key beside the database or call plaintext SQLite encrypted. |
| Wayland data control | Prefer a native data-control protocol backend. | `wl-clipboard` may be a visible subprocess fallback or diagnostic, never an invisible permanent dependency. | GNOME may be capture-on-summon; unsupported sessions stay visibly unsupported. |
| Desktop portals | Use the portal for compositor-governed shortcuts and permissions. | Add a thin `ashpd`-style adapter when the native backend is implemented. | Missing portal capability is a product capability state, not a reason to hand-roll around compositor security. |
| SQLCipher | Build and verify SQLCipher with a key from the OS provider. | Pin/fork build integration only when upstream packaging cannot produce reproducible artifacts. | Plain bundled SQLite is development-only and remains a release blocker. |

## Sequencing consequence

The current resident loop is retained as the historical tracer bullet. New work proceeds risk-first: shared fidelity oracle -> real Wayland evidence -> first native backend -> early package smoke -> privacy floor -> optional recipes -> data-contract freeze -> daemon/CLI/sync consumers. The 14-day dogfood and real compositor reports remain open acceptance evidence even though their schemas and decision logic are implemented.
