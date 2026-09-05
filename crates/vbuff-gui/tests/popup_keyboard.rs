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

#[test]
fn full_history_result_can_be_selected_and_stale_result_is_ignored() {
    let shared = Arc::new(Mutex::new(AppState {
        show_requested: true,
        ..AppState::default()
    }));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 620.0))
        .build_eframe(|_| PopupApp::new(shared.clone()));
    harness.run_steps(2);
    harness.event(egui::Event::Text("elephant".into()));
    harness.run_steps(2);
    let old = text_clip("elephant");
    shared.lock().unwrap().history_search = Some(vbuff_gui::HistorySearchResults {
        query: "previous query".into(),
        scope: vbuff_gui::experience::HistoryScope::All,
        history_revision: 0,
        clips: vec![old.clone()].into(),
        total: 1,
        failed: false,
    });
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(1);
    assert!(harness.state_mut().take_actions().is_empty());

    shared.lock().unwrap().history_search = Some(vbuff_gui::HistorySearchResults {
        query: "elephant".into(),
        scope: vbuff_gui::experience::HistoryScope::All,
        history_revision: 0,
        clips: vec![old.clone()].into(),
        total: 1,
        failed: false,
    });
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(1);
    assert_eq!(
        harness.state_mut().take_actions(),
        vec![UiAction::Paste(old.id)]
    );
}

#[test]
fn saved_search_restores_query_and_scope_through_the_menu() {
    use egui_kittest::kittest::Queryable;
    let (state, _) = populated_state();
    let mut harness = popup_harness(state);
    harness
        .state_mut()
        .set_preferences(vbuff_gui::UiPreferences {
            saved_searches: vec![vbuff_gui::experience::SavedSearch {
                name: "Work commands".into(),
                query: "cargo today".into(),
                scope: vbuff_gui::experience::HistoryScope::Snippets,
            }],
            ..Default::default()
        });
    harness.run_steps(2);
    harness.get_by_label("Saved searches").click();
    harness.run_steps(2);
    harness.get_by_label("Work commands").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().history_search_request(),
        Some((
            "cargo today".into(),
            vbuff_gui::experience::HistoryScope::Snippets
        ))
    );
}

#[test]
fn tag_manager_assigns_selection() {
    use egui_kittest::kittest::Queryable;
    let (mut state, ids) = populated_state();
    state.tags = Arc::new(vbuff_types::TagSnapshot {
        tags: vec![vbuff_types::TagRecord {
            id: "work".into(),
            name: "work".into(),
            color: [40, 140, 220],
            clips: [ids[0]].into_iter().collect(),
        }],
    });
    let mut harness = popup_harness(state);
    harness.run_steps(2);
    harness.get_by_label("Select for tagging").click();
    harness.run_steps(2);
    harness.get_by_label("Tags").click();
    harness.run_steps(2);
    harness.get_by_label("Assign").click();
    harness.run_steps(2);
    assert!(harness.state_mut().take_actions().into_iter().any(|action| matches!(action,
        UiAction::EditTags(vbuff_types::TagCommand::Assign { clips, tag, assigned: true }) if clips == vec![ids[0]] && tag == "work")));
}

#[test]
fn tag_filter_requests_full_history() {
    use egui_kittest::kittest::Queryable;
    let (mut state, ids) = populated_state();
    state.tags = Arc::new(vbuff_types::TagSnapshot {
        tags: vec![vbuff_types::TagRecord {
            id: "work".into(),
            name: "work".into(),
            color: [40, 140, 220],
            clips: [ids[0]].into_iter().collect(),
        }],
    });
    let mut harness = popup_harness(state);
    harness.run_steps(2);
    harness.get_by_label("Filter tags").click();
    harness.run_steps(2);
    harness.get_by_label("work").click();
    harness.run_steps(2);
    assert!(
        matches!(harness.state().history_search_request(), Some((_, vbuff_gui::experience::HistoryScope::Tagged { ids, all: true, .. })) if ids == vec!["work"])
    );
    harness.get_by_label("Filter tags").click();
    harness.run_steps(2);
    harness.get_by_value("All kinds").click();
    harness.run_steps(5);
    harness.get_by_label("Text").click_accesskit();
    harness.run_steps(5);

    assert!(matches!(harness.state().history_search_request(),
        Some((_, vbuff_gui::experience::HistoryScope::Tagged { base, ids, all: true }))
            if *base == vbuff_gui::experience::HistoryScope::Kind(vbuff_types::ContentKind::Text)
                && ids == vec!["work"]));
}

#[test]
fn row_menu_sets_expiry() {
    use egui_kittest::kittest::Queryable;
    let clip = text_clip("expires later");
    let id = clip.id;
    let mut state = AppState::with_clips(vec![clip]);
    state.show_requested = true;
    let mut harness = popup_harness(state);
    harness.run_steps(2);
    harness.get_by_label("Clip actions").click();
    harness.run_steps(2);
    harness.get_by_label("Set expiry").click();
    harness.run_steps(2);
    harness.get_by_label("1 hour").click();
    harness.run_steps(2);
    assert!(
        harness
            .state_mut()
            .take_actions()
            .contains(&UiAction::SetTtl(id, Some(3600)))
    );
}

#[test]
fn hidden_popup_can_process_activation_without_a_ui_pass() {
    let shared = Arc::new(Mutex::new(AppState::default()));
    let mut popup = PopupApp::new(shared.clone());
    let ctx = egui::Context::default();
    for _ in 0..3 {
        ctx.begin_pass(Default::default());
        popup.request_hide(&ctx);
        let _ = ctx.end_pass();
        shared.lock().unwrap().request_show();
        ctx.begin_pass(Default::default());
        popup.process_activation(&ctx);
        let output = ctx.end_pass();
        assert!(!shared.lock().unwrap().show_requested);
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Visible(true)))
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Focus))
        );
    }
}
