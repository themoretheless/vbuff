# Native egui design review

Reviewed on 2026-07-22. This is the decision record for the native-only popup design pass. It covers the resident `eframe`/`egui` product; it does not reintroduce a web or WASM surface.

## Ten-role panel

Ten independent read-only reviews examined the same running product and code from different design responsibilities. The implementation was then synthesized locally so no reviewer owned or silently broadened the product architecture.

| Role | Primary question |
|---|---|
| 1. Visual hierarchy | Can the selected clip and primary action be found before status chrome? |
| 2. Interaction design | Are keyboard, pointer, completion, and selection behaviors predictable? |
| 3. Native desktop conventions | Do close, tray, menu, focus, and quit semantics match a resident tool? |
| 4. Accessibility | Are contrast, focus, roles, target sizes, and reduced motion credible? |
| 5. Information architecture | Are History, Stack, Privacy, and Settings separated by user intent? |
| 6. Clipboard workflow | Is copy, paste, compose, recall, and recovery efficient under repetition? |
| 7. Privacy and trust | Do labels state observed evidence without implying unproven protection? |
| 8. State design | Are empty, paused, degraded, failed, and partial-success states actionable? |
| 9. Design-system critique | Are type, spacing, controls, icons, colors, and radii governed by tokens? |
| 10. Red-team critique | Where can the UI mislead, trap, lose state, or execute the wrong clip? |

## Consensus

1. History is the hot path. Status must remain visible but cannot dominate the selected clip and Copy/Paste action.
2. The primary navigation is `History | Stack`. Privacy and Settings are secondary destinations available through status and actions.
3. `Enter` always executes the selected clip. Search completion uses `Tab` or `Right Arrow`; suggestions never steal Enter.
4. A single click selects and updates preview. A double click executes. Selection is preserved by `ClipId`, not by a row index that can drift after capture or filtering.
5. One global command result is visible on every surface. History shows at most one operational alert at a time and indicates additional alerts compactly.
6. Secondary text must meet WCAG AA, meaningful non-text boundaries must meet 3:1, and selected rows need a non-color-only structural marker.
7. Trust copy must distinguish configuration estimates, runtime evidence, and release verification. A local hash chain is not proof of OS or storage protection.
8. The window may hide on close only when a resident tray surface exists. An explicit `Quit vbuff` action is always available.
9. Stack is a task surface, not an advanced mode hidden behind Compose terminology. Its controls must expose copy/paste, reorder, duplicate, and delete without nested cards.
10. Stable visual evidence must include the minimum window, the normal popup, a wide preview, light/dark themes, and fractional DPI.

## Three iterations

### Iteration 1: workflow and hierarchy

- Reduced primary tabs to History and Stack; moved Privacy and Settings into status/actions.
- Made the selected row and Copy/Paste action visually primary.
- Separated single-click selection from double-click execution.
- Preserved selection by stable clip identity and connected listbox active-descendant semantics.
- Kept Stack, Privacy, and Settings open across ordinary focus changes; transient History still follows popup behavior.

### Iteration 2: design system and state language

- Added shared spacing, control, radius, typography, semantic color, and icon-button variants.
- Added explicit primary, ghost, toolbar, and danger control treatments.
- Raised secondary text and boundary contrast and added automated contrast assertions.
- Rewrote empty, paused, health, partial-success, and Stack-empty states around the next useful action.
- Replaced ambiguous badges such as `Verified`, `Lossless`, and `Local` with observed claims such as `Bytes checked`, `Read succeeded`, and `No sync`.

### Iteration 3: trust and failure critique

- Capped the privacy posture in `Needs attention` unless encrypted storage and sensitive-memory-only handling are both active, with an explicit factor explaining the gate.
- Renamed session protection as a capacity-cleanup exception and stated that expiry and manual deletion still apply.
- Reframed the Privacy surface as a configuration estimate plus current-session evidence, with explicit non-guarantee language.
- Made copy-only fallback close the transient History popup after a successful copy; failed writes keep it visible and state that the clipboard was unchanged.
- Removed the preview fade so selected content is immediately legible and deterministic in visual tests.
- Added native close fallback and an unconditional Quit command so tray failure cannot leave an invisible process with no exit path.
- Closed the final critic's five findings: tray-aware hide/quit routing, exact virtual-row geometry, a keyboard-focusable Privacy status button, selected-row metadata contrast, and a single-line preview-transform picker. The verification pass reported no remaining P0/P1 blocker in that set.

## Verification matrix

The checked-in WGPU matrix contains 28 stable images:

- 24 surface/theme/DPI combinations at `560x620` and `1x`/`2x`.
- Two populated wide-preview images at `820x620` and `1x`.
- Two alert-heavy minimum-window images at `520x420` and `1.5x`.

These images prove deterministic egui layout only. They do not prove native compositor behavior, real assistive-technology output, OS focus restoration, or platform clipboard privacy.

## Deferred release gates

- The generic capture backend cannot prove source identity, concealed/transient hints, or arbitrary phrase secrets. Source-dependent policy and a green native privacy claim remain blocked on native adapters.
- SQLite remains unencrypted until the SQLCipher and key-provider path is shipped; the UI must not imply otherwise.
- Automatic paste remains disabled until the destination is confirmed immediately before native injection.
- Large-history search still needs a store-backed projection rather than loading a bounded in-memory snapshot.
- Real screen-reader, keyboard-only, native DPI, tray-loss, lock/idle, and compositor sessions require target-OS evidence before release.
- A future native hotkey recorder, caret-aware placement, and separate viewport architecture need platform prototypes; they are not simulated in the current UI.
