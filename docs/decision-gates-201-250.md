# Decision gates for batch 201-250

Reviewed on 2026-07-20. These gates prevent tested contracts from becoming claims about native behavior, sandboxing, accessibility, or encrypted recovery before the required evidence exists.

| Backlog | Gate | Continue rule | Blocked claim / fallback | Required evidence |
|---:|---|---|---|---|
| 205-207 | Pipeline execution | The same typed pipeline passes dry-run and committed execution under an exact per-action grant in a bounded host. | Keep the graph/dry-run as a foundation; run no shell, filesystem, environment, or network action. | Host conformance tests, timeout/resource proof, and grant receipt. |
| 225, 239 | Assistive technology | Paste, settings, palette, row actions, focus, and announcements pass NVDA, VoiceOver, and Orca checks. | Claim AccessKit semantics and keyboard reachability only, not screen-reader parity. | Per-platform semantic tree plus manual AT transcript. |
| 226 | Caret placement | A native adapter returns current caret bounds without stealing focus and placement passes DPI/multi-monitor tests. | Use cursor anchoring and work-area clamping. | macOS AX, Windows UIA, X11/Wayland capability reports and screenshots. |
| 233, 237, 238 | Display adaptation | Theme, DPR, viewport, forced colors, RTL/CJK shaping, and monitor transitions pass the visual matrix. | Retain logical sizing and the diagnostic gallery; do not claim full international rendering conformance. | Native screenshots/tree output across 1x/2x and forced-color modes. |
| 249 | Encrypted grace bin | A 256-bit key is obtained from an OS secret provider, never written beside SQLite, and survives approved restart/lock transitions. | Use only five-second in-memory Undo; never persist plaintext recovery data or an app-config key. | Key lifecycle state test, at-rest canary scan, restart restore, wrong-key/tamper tests. |
| 250 | Retention activation | Settings shows exact impact, grace-key availability is affirmative, pins/favorites remain protected, and synthetic retention fixtures pass. | Persist rules but defer non-zero-grace deletion when no key is supplied; sensitive hard TTL stays independent. | Preview/commit equivalence, model test, disk/CAS residue scan. |

Missing native or operational evidence is not inferred from unit tests. The resident app remains honest about cursor fallback, unhosted plugins, and unavailable durable recovery.
