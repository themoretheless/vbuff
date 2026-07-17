# vbuff - 100 High-Signal Repository Reviews

This is a curated engineering sample, not a global GitHub popularity chart. Repositories were selected for direct relevance to clipboard capture, desktop interaction, Rust platform work, local search, privacy, local-first sync, accessibility, testing, and release engineering. Star counts were verified through the GitHub GraphQL API on 2026-07-18 and are a point-in-time signal, not a quality guarantee. The smallest project in the sample has about 650 stars; most have thousands or tens of thousands.

The catalog uses stable evidence ids (`GH-001` through `GH-100`). New backlog items 501-600 in [ideas-501-600.md](ideas-501-600.md) cite these ids and the primary materials below instead of treating popularity as proof.

## Clipboard Managers and Launchers

| ID | Repository | Stars | What vbuff should learn |
|---|---|---:|---|
| GH-001 | [hluk/CopyQ](https://github.com/hluk/CopyQ) | 12,010 | Keep the core history fast and understandable; isolate scripting and commands behind explicit capabilities. |
| GH-002 | [Clipy/Clipy](https://github.com/Clipy/Clipy) | 8,778 | A native menu-bar workflow wins through low summon latency, compact rows, and predictable keyboard behavior. |
| GH-003 | [EcoPasteHub/EcoPaste](https://github.com/EcoPasteHub/EcoPaste) | 7,190 | Study a modern Rust/Tauri clipboard product, especially its cross-platform tradeoffs and rich-content model. |
| GH-004 | [sabrogden/Ditto](https://github.com/sabrogden/Ditto) | 6,754 | Mine years of Windows clipboard-format, database, hotkey, and peer-sync edge cases without inheriting its UI density. |
| GH-005 | [Slackadays/Clipboard](https://github.com/Slackadays/Clipboard) | 5,845 | Preserve a composable CLI surface and stream-friendly operations alongside the resident GUI. |
| GH-006 | [CrossPaste/crosspaste-desktop](https://github.com/CrossPaste/crosspaste-desktop) | 2,235 | Compare real cross-platform pairing, device discovery, rich formats, and transfer failure states. |
| GH-007 | [PasteBar/PasteBarApp](https://github.com/PasteBar/PasteBarApp) | 2,116 | Treat collections, actions, and polished local workflows as separate layers over history. |
| GH-008 | [sentriz/cliphist](https://github.com/sentriz/cliphist) | 1,502 | Keep Wayland capture, durable history, and picker integration as small composable processes. |
| GH-009 | [Keruspe/GPaste](https://github.com/Keruspe/GPaste) | 914 | Separate the long-lived daemon, D-Bus API, shell integration, and UI clients cleanly. |
| GH-010 | [bugaevc/wl-clipboard](https://github.com/bugaevc/wl-clipboard) | 2,381 | Reuse protocol-correct Wayland primitives and understand offer lifetime and seat semantics. |
| GH-011 | [cdown/clipmenu](https://github.com/cdown/clipmenu) | 1,250 | A tiny storage/selection pipeline is a useful reference oracle for headless Linux behavior. |
| GH-012 | [yuzeguitarist/Deck](https://github.com/yuzeguitarist/Deck) | 1,341 | Study card-like clipboard inspection while keeping primary recall denser than a dashboard. |
| GH-013 | [sindresorhus/Pasteboard-Viewer](https://github.com/sindresorhus/Pasteboard-Viewer) | 841 | Ship a safe diagnostic inspector for pasteboard items and flavors instead of hiding format failures. |
| GH-014 | [FuzzyIdeas/Clop](https://github.com/FuzzyIdeas/Clop) | 1,583 | Run media optimization as an opt-in derived representation and never mutate the canonical capture. |
| GH-015 | [Sathvik-Rao/ClipCascade](https://github.com/Sathvik-Rao/ClipCascade) | 1,821 | Review encrypted cross-device transport, reconnect behavior, and explicit device trust. |
| GH-016 | [jedisct1/piknik](https://github.com/jedisct1/piknik) | 2,514 | Keep one-shot encrypted transfer simple, scriptable, and independent from full history replication. |
| GH-017 | [espanso/espanso](https://github.com/espanso/espanso) | 14,112 | Version configuration, isolate packages, and make per-app exclusions part of the safety model. |
| GH-018 | [Wox-launcher/Wox](https://github.com/Wox-launcher/Wox) | 27,149 | Bound plugin latency and keep query routing deterministic when many providers answer. |
| GH-019 | [Flow-Launcher/Flow.Launcher](https://github.com/Flow-Launcher/Flow.Launcher) | 15,192 | Study ranking, action menus, result stability, and plugin failure containment. |
| GH-020 | [albertlauncher/albert](https://github.com/albertlauncher/albert) | 7,960 | Keep the query core independent from frontend and extension implementations. |
| GH-021 | [Ulauncher/Ulauncher](https://github.com/Ulauncher/Ulauncher) | 4,483 | Treat extension compatibility and user-visible failure recovery as product contracts. |
| GH-022 | [oliverschwendener/ueli](https://github.com/oliverschwendener/ueli) | 4,561 | Compare cross-platform settings, search providers, keyboard navigation, and packaging. |
| GH-023 | [ospfranco/sol](https://github.com/ospfranco/sol) | 3,003 | Study a focused native launcher for macOS and its fast, restrained interaction surface. |
| GH-024 | [anyrun-org/anyrun](https://github.com/anyrun-org/anyrun) | 1,276 | Keep Wayland popup placement and provider processes explicit and independently replaceable. |
| GH-025 | [Keypirinha/Keypirinha](https://github.com/Keypirinha/Keypirinha) | 1,133 | Preserve keyboard speed, portable configuration, and a stable action vocabulary. |

## Rust Desktop and Platform Foundations

| ID | Repository | Stars | What vbuff should learn |
|---|---|---:|---|
| GH-026 | [emilk/egui](https://github.com/emilk/egui) | 29,722 | Follow upstream accessibility, viewport, texture-cache, and input semantics instead of compensating in app code. |
| GH-027 | [iced-rs/iced](https://github.com/iced-rs/iced) | 31,001 | Compare explicit state/update architecture and long-running subscription ownership. |
| GH-028 | [slint-ui/slint](https://github.com/slint-ui/slint) | 23,239 | Study design-system consistency, native accessibility, and low-resource desktop rendering. |
| GH-029 | [rust-windowing/winit](https://github.com/rust-windowing/winit) | 6,061 | Respect event-loop ownership, monitor/DPI changes, activation, sleep, and platform thread constraints. |
| GH-030 | [tauri-apps/tauri](https://github.com/tauri-apps/tauri) | 109,169 | Learn capability configuration, updater hardening, tray behavior, and cross-platform packaging boundaries. |
| GH-031 | [lapce/lapce](https://github.com/lapce/lapce) | 38,664 | Study a large Rust desktop app's command routing, state boundaries, text workload, and startup costs. |
| GH-032 | [zed-industries/zed](https://github.com/zed-industries/zed) | 87,149 | Review high-performance text rendering, async task ownership, collaboration, and deterministic commands. |
| GH-033 | [1Password/arboard](https://github.com/1Password/arboard) | 954 | Treat the current fallback backend as a narrow portability baseline, not proof of native format fidelity. |
| GH-034 | [AccessKit/accesskit](https://github.com/AccessKit/accesskit) | 1,488 | Model semantic nodes and actions as testable application state, not visual afterthoughts. |
| GH-035 | [Smithay/smithay](https://github.com/Smithay/smithay) | 3,109 | Understand compositor-side Wayland ownership, seats, selections, and protocol capability boundaries. |
| GH-036 | [Smithay/wayland-rs](https://github.com/Smithay/wayland-rs) | 1,398 | Use generated protocol types and explicit object lifetimes at the Wayland boundary. |
| GH-037 | [xremap/xremap](https://github.com/xremap/xremap) | 2,134 | Study input permissions, compositor differences, device hotplug, and remapping failure modes. |
| GH-038 | [microsoft/windows-rs](https://github.com/microsoft/windows-rs) | 12,570 | Keep Win32 wrappers thin, typed, and localized around ownership and thread-affinity rules. |
| GH-039 | [madsmtm/objc2](https://github.com/madsmtm/objc2) | 983 | Use ownership-aware Objective-C bindings rather than scattering unsafe AppKit calls. |
| GH-040 | [z-galaxy/zbus](https://github.com/z-galaxy/zbus) | 721 | Keep D-Bus/portal contracts typed, versioned, cancellable, and timeout-bounded. |
| GH-041 | [flatpak/xdg-desktop-portal](https://github.com/flatpak/xdg-desktop-portal) | 815 | Treat portal grants and restore tokens as environment-scoped capabilities that can expire. |
| GH-042 | [pop-os/cosmic-text](https://github.com/pop-os/cosmic-text) | 2,111 | Use a real shaping pipeline for Unicode, bidi text, fallback fonts, and grapheme-safe geometry. |
| GH-043 | [linebender/parley](https://github.com/linebender/parley) | 671 | Track modern text layout and accessibility-friendly selection/highlight primitives. |
| GH-044 | [image-rs/image](https://github.com/image-rs/image) | 5,820 | Enforce decoded-pixel, dimensions, allocation, and malformed-image limits before thumbnailing. |
| GH-045 | [tokio-rs/tracing](https://github.com/tokio-rs/tracing) | 6,781 | Use structured spans and a redaction layer so diagnostics never absorb clipboard payloads. |

## Storage, Search, and Security

| ID | Repository | Stars | What vbuff should learn |
|---|---|---:|---|
| GH-046 | [sqlite/sqlite](https://github.com/sqlite/sqlite) | 10,057 | Copy SQLite's crash, fault-injection, integrity, and version-aware discipline around the actual storage mode. |
| GH-047 | [sqlcipher/sqlcipher](https://github.com/sqlcipher/sqlcipher) | 7,209 | Treat cipher parameters, migration, key rotation, and plaintext verification as explicit versioned contracts. |
| GH-048 | [rusqlite/rusqlite](https://github.com/rusqlite/rusqlite) | 4,308 | Track the bundled SQLite version separately from the Rust wrapper version and test feature changes. |
| GH-049 | [quickwit-oss/tantivy](https://github.com/quickwit-oss/tantivy) | 15,559 | Compare tokenization, segment maintenance, relevance testing, and deterministic ranking. |
| GH-050 | [meilisearch/meilisearch](https://github.com/meilisearch/meilisearch) | 58,629 | Study typo tolerance and ranking rules, but keep vbuff's local index small and explainable. |
| GH-051 | [typesense/typesense](https://github.com/typesense/typesense) | 26,320 | Make typo tolerance bounded by field, token length, and exact-match priority. |
| GH-052 | [tursodatabase/libsql](https://github.com/tursodatabase/libsql) | 16,968 | Learn replication and embedded-database tradeoffs without replacing a simpler local-only store prematurely. |
| GH-053 | [spacejam/sled](https://github.com/spacejam/sled) | 9,045 | Treat crash consistency claims as obligations backed by fault tests and a conservative recovery story. |
| GH-054 | [fjall-rs/fjall](https://github.com/fjall-rs/fjall) | 2,221 | Review modern Rust LSM design, compaction, snapshots, and recovery as comparative evidence. |
| GH-055 | [keepassxreboot/keepassxc](https://github.com/keepassxreboot/keepassxc) | 28,035 | Study vault locking, clipboard expiry, memory handling, security settings, and user-visible trust. |
| GH-056 | [bitwarden/clients](https://github.com/bitwarden/clients) | 13,306 | Review cross-platform secret UX, device trust, autofill boundaries, and release hardening. |
| GH-057 | [gitleaks/gitleaks](https://github.com/gitleaks/gitleaks) | 28,183 | Use detector ids, allowlists, entropy, fixtures, and measurable false-positive control. |
| GH-058 | [trufflesecurity/trufflehog](https://github.com/trufflesecurity/trufflehog) | 27,082 | Separate candidate detection from optional verification and never make network verification implicit. |
| GH-059 | [Infisical/infisical](https://github.com/Infisical/infisical) | 28,154 | Review key lifecycle, auditability, secret rotation, and least-privilege service boundaries. |
| GH-060 | [FiloSottile/age](https://github.com/FiloSottile/age) | 22,902 | Prefer small, misuse-resistant cryptographic formats and explicit recipient identity. |
| GH-061 | [jedisct1/libsodium](https://github.com/jedisct1/libsodium) | 13,812 | Use high-level authenticated-encryption APIs, guarded memory where justified, and published test vectors. |
| GH-062 | [rustls/rustls](https://github.com/rustls/rustls) | 7,524 | Keep secure defaults, protocol state machines, and certificate/key handling away from product logic. |
| GH-063 | [RustCrypto/AEADs](https://github.com/RustCrypto/AEADs) | 947 | Pin algorithms and nonce rules through interoperable vectors instead of inventing envelope crypto. |
| GH-064 | [getsops/sops](https://github.com/getsops/sops) | 22,527 | Separate encrypted payloads from key recipients and make key rotation auditable. |
| GH-065 | [OWASP/mastg](https://github.com/OWASP/mastg) | 13,057 | Turn the threat model into repeatable platform checks rather than prose-only security claims. |

## Sync and Local-First Systems

| ID | Repository | Stars | What vbuff should learn |
|---|---|---:|---|
| GH-066 | [syncthing/syncthing](https://github.com/syncthing/syncthing) | 86,557 | Study durable device identity, discovery, conflict handling, relay fallback, and honest status. |
| GH-067 | [localsend/localsend](https://github.com/localsend/localsend) | 85,434 | Keep nearby transfer discoverable and simple while authenticating peers and surfacing network failures. |
| GH-068 | [KDE/kdeconnect-kde](https://github.com/KDE/kdeconnect-kde) | 3,860 | Review clipboard loops, device permissions, plugin isolation, and desktop/mobile lifecycle mismatch. |
| GH-069 | [schollz/croc](https://github.com/schollz/croc) | 35,582 | Use PAKE-like short-code pairing, relay fallback, resume, and one-shot transfer semantics. |
| GH-070 | [magic-wormhole/magic-wormhole](https://github.com/magic-wormhole/magic-wormhole) | 22,717 | Bind human-readable pairing codes to authenticated key exchange and explicit completion. |
| GH-071 | [libp2p/rust-libp2p](https://github.com/libp2p/rust-libp2p) | 5,579 | Treat discovery, transport, identity, and stream protocols as independently negotiated layers. |
| GH-072 | [yjs/yjs](https://github.com/yjs/yjs) | 22,198 | Study compact CRDT updates, state vectors, garbage collection, and offline convergence. |
| GH-073 | [automerge/automerge](https://github.com/automerge/automerge) | 6,427 | Define operations and conflicts explicitly, then prove convergence across reorder and duplication. |
| GH-074 | [loro-dev/loro](https://github.com/loro-dev/loro) | 5,890 | Compare modern CRDT version vectors, snapshots, diffing, and state compaction. |
| GH-075 | [vlcn-io/cr-sqlite](https://github.com/vlcn-io/cr-sqlite) | 3,741 | Review row-level CRDT semantics while keeping raw database-file sync forbidden. |
| GH-076 | [anyproto/any-sync](https://github.com/anyproto/any-sync) | 1,660 | Study local-first account/device identity, encrypted spaces, and peer topology. |
| GH-077 | [tailscale/tailscale](https://github.com/tailscale/tailscale) | 33,994 | Learn NAT traversal, key rotation, network-state UX, and conservative connectivity fallback. |
| GH-078 | [zerotier/ZeroTierOne](https://github.com/zerotier/ZeroTierOne) | 16,935 | Review virtual-network identity and topology, but avoid importing network complexity into MVP sync. |
| GH-079 | [rclone/rclone](https://github.com/rclone/rclone) | 58,389 | Study resumable transfers, checksums, bandwidth policies, and provider failure taxonomy. |
| GH-080 | [restic/restic](https://github.com/restic/restic) | 35,039 | Use content-addressed encrypted chunks, authenticated snapshots, retention, and restore verification. |
| GH-081 | [borgbackup/borg](https://github.com/borgbackup/borg) | 13,523 | Study chunking, deduplication, repository integrity, compaction, and interrupted recovery. |
| GH-082 | [pubkey/rxdb](https://github.com/pubkey/rxdb) | 23,273 | Compare local-first replication checkpoints, conflict handlers, schema migration, and offline state. |
| GH-083 | [tinyplex/tinybase](https://github.com/tinyplex/tinybase) | 5,124 | Keep reactive local state, derived indexes, persistence, and synchronization as separable modules. |
| GH-084 | [orbitinghail/sqlsync](https://github.com/orbitinghail/sqlsync) | 2,905 | Review local SQLite views over synchronized operations without assuming file-level replication. |
| GH-085 | [TryQuiet/quiet](https://github.com/TryQuiet/quiet) | 2,620 | Study peer identity, Tor/offline behavior, local-first UX, and the support cost of complex networking. |

## Accessibility, Testing, and Release Engineering

| ID | Repository | Stars | What vbuff should learn |
|---|---|---:|---|
| GH-086 | [nvaccess/nvda](https://github.com/nvaccess/nvda) | 2,603 | Test real focus, name, role, value, live updates, and list navigation with an actual screen reader. |
| GH-087 | [dequelabs/axe-core](https://github.com/dequelabs/axe-core) | 7,315 | Encode deterministic accessibility rules, while retaining manual assistive-technology review. |
| GH-088 | [OptiKey/OptiKey](https://github.com/OptiKey/OptiKey) | 4,402 | Provide large targets, dwell/switch alternatives, and workflows that do not require precise pointing. |
| GH-089 | [houmain/keymapper](https://github.com/houmain/keymapper) | 1,123 | Test layout-independent keys, held modifiers, device changes, and synthetic-input edge cases. |
| GH-090 | [microsoft/playwright](https://github.com/microsoft/playwright) | 93,034 | Copy deterministic isolation, tracing, retries as diagnostics, and artifact-rich failure reports. |
| GH-091 | [appium/appium](https://github.com/appium/appium) | 21,775 | Drive real native surfaces through stable semantic selectors instead of pixel coordinates. |
| GH-092 | [google/oss-fuzz](https://github.com/google/oss-fuzz) | 12,441 | Run parsers continuously with sanitizer coverage and retain minimized regression inputs. |
| GH-093 | [rust-fuzz/cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) | 1,851 | Keep focused fuzz targets for every untrusted clipboard and archive parser. |
| GH-094 | [EmbarkStudios/cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | 2,369 | Make license, advisory, source, and duplicate-version policy executable in CI. |
| GH-095 | [rustsec/rustsec](https://github.com/rustsec/rustsec) | 1,925 | Track advisories continuously and record why any exception is acceptable. |
| GH-096 | [mozilla/cargo-vet](https://github.com/mozilla/cargo-vet) | 965 | Apply review criteria by dependency risk instead of treating every crate as equally trusted. |
| GH-097 | [axodotdev/cargo-dist](https://github.com/axodotdev/cargo-dist) | 2,070 | Standardize signed artifacts, installers, checksums, manifests, and release smoke inputs. |
| GH-098 | [taiki-e/cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | 1,423 | Measure branch coverage on privacy, migration, and recovery paths, not only aggregate line coverage. |
| GH-099 | [sourcefrog/cargo-mutants](https://github.com/sourcefrog/cargo-mutants) | 1,220 | Prove that tests fail when capture policy, retention, or redaction decisions are inverted. |
| GH-100 | [release-plz/release-plz](https://github.com/release-plz/release-plz) | 1,420 | Make dependency/release changes reviewable PRs with generated changelog evidence. |

## Primary Research and Standards

These sources were read for mechanisms and failure modes. A proposal cites them as `S-xx`; a standard or paper does not automatically justify shipping a feature.

| ID | Primary source | Finding applied to vbuff |
|---|---|---|
| S-01 | [Local-first software: You own your data, in spite of the cloud](https://www.inkandswitch.com/essay/local-first/) | Offline availability and user ownership are product properties; CRDTs help, but conflict and collaboration UX remain explicit design work. |
| S-02 | [A Conflict-Free Replicated JSON Datatype](https://arxiv.org/abs/1608.03960) | A client-side operation model can converge without network ordering, which suits peer-to-peer and end-to-end encrypted replication. |
| S-03 | [Conflict-free Replicated Data Types survey](https://arxiv.org/abs/1805.06358) | Convergence depends on a precise datatype and assumptions, not on applying a generic last-write-wins label. |
| S-04 | [PushPin: a production peer-to-peer collaboration experiment](https://www.inkandswitch.com/pushpin/) | NAT traversal, peer availability, and understandable connection states remain hard even when the data model is local-first. |
| S-05 | [Disrupting Continuity of Apple's Wireless Ecosystem](https://www.usenix.org/conference/usenixsecurity21/presentation/stute) | Universal Clipboard-style discovery and transport can expose tracking, denial-of-service, and machine-in-the-middle surfaces. |
| S-06 | [EthClipper: A Clipboard Meddling Attack on Ethereum Users](https://arxiv.org/abs/2108.14004) | Verifying only the visible prefix of a long address is unsafe; comparison UI must expose the full high-risk value or a trusted checksum. |
| S-07 | [How Bad Can It Git?](https://www.ndss-symposium.org/ndss-paper/how-bad-can-it-git-characterizing-secret-leakage-in-public-github-repositories/) | Secret leakage is frequent enough that detector quality, calibration, and safe defaults deserve first-class tests. |
| S-08 | [Stack Overflow Considered Helpful](https://www.usenix.org/conference/usenixsecurity19/presentation/fischer) | A warning is not success by itself; security nudges should be evaluated by safer user behavior. |
| S-09 | [Unicode Bidirectional Algorithm, UAX #9](https://www.unicode.org/reports/tr9/) | Stored logical order, displayed visual order, and directional isolation must be handled deliberately, especially for code and identifiers. |
| S-10 | [Unicode Normalization Forms, UAX #15](https://www.unicode.org/reports/tr15/) | Search normalization must be a derived representation; canonical clipboard bytes must remain untouched. |
| S-11 | [Unicode Text Segmentation, UAX #29](https://www.unicode.org/reports/tr29/) | Highlight, truncation, cursor movement, and selection must operate on grapheme boundaries rather than byte or scalar offsets. |
| S-12 | [Unicode Security Mechanisms, UTS #39](https://www.unicode.org/reports/tr39/) | Confusable detection is useful for identifier-risk signals, but it is heuristic and should not rewrite or broadly block ordinary text. |
| S-13 | [The Probabilistic Relevance Framework: BM25 and Beyond](https://www.nowpublishers.com/article/DownloadEBook/INR-019) | Ranking needs an explicit probabilistic model, relevance fixtures, and deterministic tie-breaking, not an unexplained score soup. |
| S-14 | [A New Derivation and Dataset for Fitts' Law of Human Motion](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2013/EECS-2013-171.html) | Target distance and size affect pointing time; dense icon rails still need adequate hit regions and non-pointer alternatives. |
| S-15 | [WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Focus visibility, target size, non-drag alternatives, names, roles, and values provide concrete review criteria for the popup and settings. |
| S-16 | [Microsoft Accessibility Testing](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility-testing) | Automated accessibility-tree checks and manual assistive-technology testing are complementary release gates. |
| S-17 | [Clipboard API and Events](https://www.w3.org/TR/clipboard-apis/) | Clipboard access is a powerful capability; remote clipboard and multi-format behavior require explicit permission and processing models. |
| S-18 | [Windows Clipboard Formats](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats) | A clipboard item can expose ordered native, private, and synthesized formats, plus explicit do-not-history/cloud markers. |
| S-19 | [Apple NSPasteboard](https://developer.apple.com/documentation/appkit/nspasteboard/) | A pasteboard can contain multiple items and multiple types; flattening that topology loses fidelity. |
| S-20 | [Wayland ext-data-control-v1](https://wayland.app/protocols/ext-data-control-v1) | Clipboard management is privileged, per-seat, object-lifetime-sensitive, and still protocol/version dependent. |
| S-21 | [SQLite Write-Ahead Logging](https://www.sqlite.org/wal.html) | WAL is persistent state that must travel with the database; current SQLite guidance also documents a rare fixed WAL-reset corruption race. |
| S-22 | [Atomic Commit in SQLite](https://www.sqlite.org/atomiccommit.html) | Power-loss confidence comes from a faulting VFS that injects torn, reordered, and incomplete writes at many transaction points. |
| S-23 | [Noise Protocol Framework](https://noiseprotocol.org/noise.html) | Authentication, forward secrecy, identity hiding, and transport keys must be selected through a named, testable handshake pattern. |
| S-24 | [RFC 9382: SPAKE2](https://www.rfc-editor.org/rfc/rfc9382.html) | A short shared pairing code can derive a strong key without exposing the code; transcript and key confirmation are required. |
| S-25 | [RFC 9000: QUIC](https://www.rfc-editor.org/rfc/rfc9000.html) | Fast transport includes replay-sensitive 0-RTT semantics; mutation operations need a stricter policy than idempotent reads. |
| S-26 | [Windows Clipboard Operations](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-operations) | Clipboard ownership, delayed rendering, ordered formats, and short lock duration require a bounded native state machine. |

## Concrete Audit Finding

The review began with `rusqlite 0.37.0` and `libsqlite3-sys 0.35.0`, whose bundled SQLite constant was `3.50.2`. SQLite's WAL documentation says the rare WAL-reset corruption race is fixed in `3.50.7`, `3.51.3`, and later. The review therefore upgraded vbuff to `rusqlite 0.40.1` and `libsqlite3-sys 0.38.1`, bundling SQLite `3.53.2`, disabled unneeded default features, and added a runtime-version regression test. Item 581 keeps that guard as an ongoing release invariant; item 582 proposes the deeper concurrency reproducer.

## Selection Limits

- Stars measure attention, not correctness, maintenance quality, security, accessibility, or architectural fit.
- The catalog intentionally mixes direct competitors with component references; vbuff should borrow mechanisms, not product scope.
- No dependency is approved by appearing here. Adoption still requires license, advisory, maintenance, unsafe-code, size, and replacement-cost review.
- Repository lessons are snapshots. `GH-*` evidence must be rechecked before a milestone promotes an item into committed work.
