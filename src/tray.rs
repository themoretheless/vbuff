//! Menu-bar / system-tray surface.

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::commands::AppCommand;

/// Owns the menu-bar icon, menu items, and event-id mapping.
pub(crate) struct Tray {
    _icon: TrayIcon,
    show_id: MenuId,
    copy_latest_id: MenuId,
    clear_history_id: MenuId,
    pause_id: MenuId,
    autostart_id: MenuId,
    quit_id: MenuId,
    copy_latest: MenuItem,
    clear_history: MenuItem,
    pause: MenuItem,
    autostart: MenuItem,
}

impl Tray {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let menu = Menu::new();
        let show = MenuItem::new("Show vbuff", true, None);
        let copy_latest = MenuItem::new("Copy latest clip", false, None);
        let clear_history = MenuItem::new("Clear history...", false, None);
        let pause = MenuItem::new("Pause capture", true, None);
        let autostart = MenuItem::new("Start at login", true, None);
        let quit = MenuItem::new("Quit vbuff", true, None);

        menu.append(&show)?;
        menu.append(&copy_latest)?;
        menu.append(&clear_history)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&pause)?;
        menu.append(&autostart)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit)?;

        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("vbuff clipboard manager")
            .with_icon(build_icon()?);
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_as_template(true);
        let icon = builder.build()?;

        Ok(Self {
            _icon: icon,
            show_id: show.id().clone(),
            copy_latest_id: copy_latest.id().clone(),
            clear_history_id: clear_history.id().clone(),
            pause_id: pause.id().clone(),
            autostart_id: autostart.id().clone(),
            quit_id: quit.id().clone(),
            copy_latest,
            clear_history,
            pause,
            autostart,
        })
    }

    pub(crate) fn sync_state(&self, paused: bool, clip_count: usize, launch_at_login: bool) {
        self.pause.set_text(if paused {
            "Resume capture"
        } else {
            "Pause capture"
        });
        self.autostart.set_text(if launch_at_login {
            "Don't start at login"
        } else {
            "Start at login"
        });
        self.copy_latest.set_enabled(clip_count > 0);
        self.clear_history.set_enabled(clip_count > 0);
    }

    pub(crate) fn poll(&self) -> Vec<AppCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let command = if event.id == self.show_id {
                Some(AppCommand::Show)
            } else if event.id == self.copy_latest_id {
                Some(AppCommand::CopyLatest)
            } else if event.id == self.clear_history_id {
                Some(AppCommand::RequestClearHistory)
            } else if event.id == self.pause_id {
                Some(AppCommand::TogglePause)
            } else if event.id == self.autostart_id {
                Some(AppCommand::ToggleAutostart)
            } else if event.id == self.quit_id {
                Some(AppCommand::Quit)
            } else {
                None
            };

            if let Some(command) = command {
                commands.push(command);
            }
        }
        commands
    }
}

/// A transparent clipboard/check glyph that survives light and dark menu bars.
fn build_icon() -> anyhow::Result<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(build_icon_rgba(), 32, 32)
        .map_err(|error| anyhow::anyhow!("building tray icon: {error}"))
}

fn build_icon_rgba() -> Vec<u8> {
    const SIZE: usize = 32;
    let mut rgba = vec![0; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let body = ((7..=24).contains(&x) && matches!(y, 8 | 9 | 26 | 27))
                || ((8..=27).contains(&y) && matches!(x, 7 | 8 | 23 | 24));
            let clip = ((12..=19).contains(&x) && matches!(y, 4 | 5 | 9 | 10))
                || ((4..=10).contains(&y) && matches!(x, 12 | 13 | 18 | 19));
            let check = near_segment(x, y, (11, 17), (14, 20), 1.35)
                || near_segment(x, y, (14, 20), (21, 13), 1.35);

            let color = if check {
                [0x42, 0xd3, 0xb2, 0xff]
            } else if body || clip {
                [0xf4, 0xf6, 0xf8, 0xff]
            } else {
                continue;
            };
            let offset = (y * SIZE + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }

    rgba
}

fn near_segment(
    x: usize,
    y: usize,
    start: (usize, usize),
    end: (usize, usize),
    radius: f32,
) -> bool {
    let point = (x as f32, y as f32);
    let start = (start.0 as f32, start.1 as f32);
    let end = (end.0 as f32, end.1 as f32);
    let delta = (end.0 - start.0, end.1 - start.1);
    let length_sq = delta.0 * delta.0 + delta.1 * delta.1;
    let projection = (((point.0 - start.0) * delta.0 + (point.1 - start.1) * delta.1) / length_sq)
        .clamp(0.0, 1.0);
    let nearest = (
        start.0 + projection * delta.0,
        start.1 + projection * delta.1,
    );
    let distance_sq = (point.0 - nearest.0).powi(2) + (point.1 - nearest.1).powi(2);
    distance_sq <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_glyph_is_sparse_and_has_transparent_edges() {
        let rgba = build_icon_rgba();
        let opaque = rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();

        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!((100..300).contains(&opaque));
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
    }
}
