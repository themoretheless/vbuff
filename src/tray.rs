//! Tray-icon support, compiled only when the `tray` feature is enabled.

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// A high-level tray action.
pub(crate) enum TrayAction {
    Show,
    CopyLatest,
    ClearHistory,
    TogglePause,
    Quit,
}

/// Owns the tray icon and its menu item ids.
pub(crate) struct Tray {
    _icon: TrayIcon,
    show_id: MenuId,
    copy_latest_id: MenuId,
    clear_history_id: MenuId,
    pause_id: MenuId,
    quit_id: MenuId,
    copy_latest: MenuItem,
    clear_history: MenuItem,
    pause: MenuItem,
}

impl Tray {
    /// Build the tray icon and menu.
    pub(crate) fn new() -> anyhow::Result<Self> {
        let menu = Menu::new();
        let show = MenuItem::new("Show vbuff", true, None);
        let copy_latest = MenuItem::new("Copy latest clip", false, None);
        let clear_history = MenuItem::new("Clear history", false, None);
        let pause = MenuItem::new("Pause capture", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&show)?;
        menu.append(&copy_latest)?;
        menu.append(&clear_history)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&pause)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        let icon = build_icon();
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("vbuff clipboard manager")
            .with_icon(icon)
            .build()?;

        Ok(Tray {
            _icon: icon,
            show_id: show.id().clone(),
            copy_latest_id: copy_latest.id().clone(),
            clear_history_id: clear_history.id().clone(),
            pause_id: pause.id().clone(),
            quit_id: quit.id().clone(),
            copy_latest,
            clear_history,
            pause,
        })
    }

    /// Keep menu labels and disabled states in sync with the app state.
    pub(crate) fn sync_state(&self, paused: bool, clip_count: usize) {
        self.pause.set_text(if paused {
            "Resume capture"
        } else {
            "Pause capture"
        });
        self.copy_latest.set_enabled(clip_count > 0);
        self.clear_history.set_enabled(clip_count > 0);
    }

    /// Drain pending tray/menu events into high-level actions.
    pub(crate) fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show_id {
                out.push(TrayAction::Show);
            } else if event.id == self.copy_latest_id {
                out.push(TrayAction::CopyLatest);
            } else if event.id == self.clear_history_id {
                out.push(TrayAction::ClearHistory);
            } else if event.id == self.pause_id {
                out.push(TrayAction::TogglePause);
            } else if event.id == self.quit_id {
                out.push(TrayAction::Quit);
            }
        }
        out
    }
}

/// A tiny solid 32x32 RGBA icon so the tray has something to show.
fn build_icon() -> tray_icon::Icon {
    const N: usize = 32;
    let mut rgba = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            // A simple rounded-ish blue square.
            let border = x < 2 || y < 2 || x >= N - 2 || y >= N - 2;
            if border {
                rgba.extend_from_slice(&[0x20, 0x40, 0x80, 0xff]);
            } else {
                rgba.extend_from_slice(&[0x3a, 0x6e, 0xd0, 0xff]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, N as u32, N as u32).expect("valid icon")
}
