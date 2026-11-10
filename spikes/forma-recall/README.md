# Spike: Recall screen on Forma

Question: can vbuff's popup be drawn with Forma (`../../../forma`) instead of
egui, on the same `AppState` / `UiAction` seam? This crate is a throwaway
answer for one screen. It is standalone (own `[workspace]`), so the main
workspace does not depend on a Forma checkout.

    cargo run --release                       # window: type, arrows, Enter, Esc
    cargo run --release -- --shot out.png     # one frame, no window
    cargo run --release -- --bench            # timings, no window
    # optional: --query <text>  --down <n>  --clips <n> (default 2000)

Needs Node.js (Forma's form generator runs from `build.rs`) and the Forma
repository next to this one, or `FORMA_ROOT`.

## What works

- `ui/Recall.ui` compiles to a typed `Recall` form bound to `RecallVm`.
  `RecallVm` reads `vbuff_gui::SharedState` and emits `UiAction::Paste`.
- Text input with two-way binding and a `changed` event drives filtering.
- Keyed list (`for row in state.rows key row.id`), hover, click with a typed
  event argument, selection highlight, multi-line preview text.
- A borderless, always-on-top 900x580 window through a small winit +
  softbuffer host (`src/window.rs`). The host owns arrows, Enter and Escape.
- Release timings on the development Mac, 19 controls in the scene:
  2,000 clips: 1.3-4.3 ms per keystroke, 12-18 ms full CPU repaint at
  1800x1160. 50,000 clips: about 7 ms per keystroke. The filter is a naive
  substring scan in the spike, not Forma code.

## Gaps found in Forma

1. `forma::native_window` cannot host a resident popup: no hide/show, no
   borderless or always-on-top option, no position, no external wake events.
   The spike bypasses it, which also drops its AccessKit and GPU paths.
2. No ellipsis. `clip: true` on a `Frame` cuts overflowing text at the cell
   edge (used in `components/Note.ui`), but nothing marks the cut.
3. `wrap: true` on `Text` wraps long lines, but the auto row height ignores
   the wrap unless the `Text` has an explicit `width`; without it the last
   wrapped line was clipped (`components/Block.ui` sets the width).
4. One bundled face (Ubuntu Light), so code does not align, and the "↵" glyph
   is missing (rendered as "?"). A monospace family (`fontFamily: 'mono'`,
   IBM Plex Mono) exists on the uncommitted `monospace-font` branch in the
   `../../../forma-wt-mono` worktree; `shots/recall-mono.png` was rendered
   against it. This crate still builds against the main Forma checkout and
   does not use the property yet.
5. A scene holds at most 256 controls, so the list is windowed by hand
   (`VISIBLE_ROWS`). Scroll was not tried.
6. A control placed directly in a scene needs a `Rectangle` root
   (`components/Line.ui` wraps the text leaf for that).
7. Implicit sizes in `Row` failed at load time ("'y' must be non-negative",
   "'width' must be positive"), including `width: *` and a `Row` with
   `column.span`. Every scene item has explicit sizes as a workaround; the
   root cause was not investigated.
8. Build chain: Node.js at build time, path dependencies into the Forma
   repository, a git dependency on the tokenizer.

## Not verified

- Typing and arrow keys in the real window. The window opened, took focus and
  exited through the host's Escape handler, but text entry and selection were
  only exercised through the runtime API (`--query`, `--down`, `--bench`).
- Images and thumbnails, IME, screen readers, Windows, Linux.
