//! Keyboard-navigation regression tests for the popup history surface.
//!
//! Black-box only: the popup is driven through egui_kittest key events and the
//! assertions read drained [`UiAction`]s, because the selection index itself is
//! private. These tests pin the focus contract behind the history shortcuts:
//! the search box must own exactly `egui::Id::new("vbuff_history_search")`,
//! otherwise every shortcut gate (`focused == history_search_id()`) stays false
//! while the search box holds focus — which is always, right after `show()`.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use vbuff_core::content_hash_from_flavors;
use vbuff_gui::{AppState, PopupApp, UiAction};
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

fn text_clip(text: &str) -> Clip {
    let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
    Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
        pinned: false,
        favorite: false,
    }
}

fn populated_state() -> (AppState, [ClipId; 3]) {
    let clips = vec![
        text_clip("first clip"),
        text_clip("second clip"),
        text_clip("third clip"),
    ];
    let ids = [clips[0].id, clips[1].id, clips[2].id];
    let mut state = AppState::with_clips(clips);
    state.show_requested = true;
    (state, ids)
}

fn popup_harness(state: AppState) -> Harness<'static, PopupApp> {
    Harness::builder()
        .with_size(egui::vec2(560.0, 620.0))
        .build_eframe(|_| PopupApp::new(Arc::new(Mutex::new(state))))
}

#[test]
fn popup_focus_lands_on_the_stable_search_id() {
    let (state, _) = populated_state();
    let mut harness = popup_harness(state);

    harness.run_steps(2);

    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("vbuff_history_search"))
    );
}

#[test]
fn arrow_down_then_enter_pastes_the_second_clip() {
    let (state, ids) = populated_state();
    let mut harness = popup_harness(state);
    harness.run_steps(2);

    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(1);

    assert_eq!(
        harness.state_mut().take_actions(),
        vec![UiAction::Paste(ids[1])]
    );
}

#[test]
fn end_then_enter_pastes_the_last_clip() {
    let (state, ids) = populated_state();
    let mut harness = popup_harness(state);
    harness.run_steps(2);

    harness.key_press(egui::Key::End);
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(1);

    assert_eq!(
        harness.state_mut().take_actions(),
        vec![UiAction::Paste(ids[2])]
    );
}

#[test]
fn command_num2_quick_selects_the_second_clip() {
    let (state, ids) = populated_state();
    let mut harness = popup_harness(state);
    harness.run_steps(2);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num2);
    harness.run_steps(1);

    assert_eq!(
        harness.state_mut().take_actions(),
        vec![UiAction::Paste(ids[1])]
    );
}
