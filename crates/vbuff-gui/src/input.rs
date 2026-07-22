//! Keyboard handling for the popup: turns key presses into selection changes
//! and queued [`UiAction`]s. No rendering and no state-snapshot logic lives
//! here - only "given these keys, what changed."

use egui::Key;
use vbuff_types::ClipId;

use crate::app::PopupApp;
use crate::state::UiAction;
use crate::theme::QUICK_PICK_SLOTS;

impl PopupApp {
    /// Handle Esc / arrow navigation / Enter / quick-pick number keys for the
    /// currently filtered row set.
    pub(crate) fn handle_keys(&mut self, ctx: &egui::Context, filtered: &[ClipId], total: usize) {
        let modifier_down = ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) {
                self.actions.push_back(UiAction::Hide);
            }
            if i.key_pressed(Key::ArrowDown) && total > 0 {
                self.selected = (self.selected + 1).min(total - 1);
            }
            if i.key_pressed(Key::ArrowUp) && total > 0 {
                self.selected = self.selected.saturating_sub(1);
            }
            if i.key_pressed(Key::Enter)
                && total > 0
                && let Some(id) = filtered.get(self.selected)
            {
                self.actions.push_back(UiAction::Paste(*id));
            }
            // Cmd/Ctrl + 1..9 quick select.
            if modifier_down {
                const QUICK_PICK_KEYS: [Key; QUICK_PICK_SLOTS] = [
                    Key::Num1,
                    Key::Num2,
                    Key::Num3,
                    Key::Num4,
                    Key::Num5,
                    Key::Num6,
                    Key::Num7,
                    Key::Num8,
                    Key::Num9,
                ];
                for (n, key) in QUICK_PICK_KEYS.into_iter().enumerate() {
                    if i.key_pressed(key)
                        && let Some(id) = filtered.get(n)
                    {
                        self.actions.push_back(UiAction::Paste(*id));
                    }
                }
            }
        });
    }
}
