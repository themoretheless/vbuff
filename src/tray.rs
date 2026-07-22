//! Menu-bar / system-tray surface.

use std::cell::Cell;
use std::time::{Duration, Instant};

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};
use vbuff_platform::{QuickMenuLabels, ResidentStatus, current_desktop_shell};
use vbuff_types::{CaptureHealth, CapturePauseReason};

use crate::commands::AppCommand;

/// Owns the menu-bar icon, menu items, and event-id mapping.
pub(crate) struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    labels: QuickMenuLabels,
    last_status: Cell<ResidentStatus>,
    paste_confirmed_until: Cell<Option<Instant>>,
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
        let labels = QuickMenuLabels::for_shell(current_desktop_shell());
        let menu = Menu::new();
        let status = MenuItem::new("Capture starting", false, None);
        let show = MenuItem::new(labels.open, true, None);
        let copy_latest = MenuItem::new(labels.copy_latest, false, None);
        let clear_history = MenuItem::new(labels.clear_history, false, None);
        let pause = MenuItem::new(labels.pause, true, None);
        let autostart = MenuItem::new(labels.autostart_on, true, None);
        let quit = MenuItem::new(labels.quit, true, None);

        menu.append(&status)?;
        menu.append(&PredefinedMenuItem::separator())?;
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
            .with_icon(build_icon(ResidentStatus::Active)?);
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_as_template(true);
        let icon = builder.build()?;

        Ok(Self {
            icon,
            status,
            labels,
            last_status: Cell::new(ResidentStatus::Active),
            paste_confirmed_until: Cell::new(None),
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

    pub(crate) fn sync_state(
        &self,
        paused: bool,
        pause_reason: Option<CapturePauseReason>,
        health: CaptureHealth,
        clip_count: usize,
        launch_at_login: bool,
        now: Instant,
    ) {
        let paste_confirmed = self
            .paste_confirmed_until
            .get()
            .is_some_and(|deadline| now < deadline);
        if !paste_confirmed {
            self.paste_confirmed_until.set(None);
        }
        self.apply_status(ResidentStatus::from_runtime(
            paused,
            pause_reason,
            health,
            paste_confirmed,
        ));
        self.status.set_text(capture_status_text(paused, health));
        self.pause.set_text(if paused {
            self.labels.resume
        } else {
            self.labels.pause
        });
        self.autostart.set_text(if launch_at_login {
            self.labels.autostart_off
        } else {
            self.labels.autostart_on
        });
        self.copy_latest.set_enabled(clip_count > 0);
        self.clear_history.set_enabled(clip_count > 0);
    }

    pub(crate) fn acknowledge_paste(&self, now: Instant) {
        self.paste_confirmed_until
            .set(now.checked_add(Duration::from_millis(650)));
        self.apply_status(ResidentStatus::PasteSent);
    }

    fn apply_status(&self, status: ResidentStatus) {
        if self.last_status.get() == status {
            return;
        }
        match build_icon(status).and_then(|icon| {
            self.icon
                .set_icon(Some(icon))
                .map_err(|error| anyhow::anyhow!(error))
        }) {
            Ok(()) => self.last_status.set(status),
            Err(error) => tracing::warn!("updating tray status icon failed: {error}"),
        }
        let tooltip = if status == ResidentStatus::PasteSent {
            "vbuff - paste shortcut sent"
        } else {
            "vbuff clipboard manager"
        };
        if let Err(error) = self.icon.set_tooltip(Some(tooltip)) {
            tracing::debug!("tray tooltip update unavailable: {error}");
        }
        let title = status.title();
        if title.is_empty() {
            self.icon.set_title::<&str>(None);
        } else {
            self.icon.set_title(Some(title));
        }
    }

    pub(crate) fn command_for(&self, event: &MenuEvent) -> Option<AppCommand> {
        if event.id == self.show_id {
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
        }
    }
}

fn capture_status_text(paused: bool, health: CaptureHealth) -> String {
    if paused {
        "Capture paused".to_owned()
    } else {
        health.label().to_owned()
    }
}

/// A transparent clipboard/check glyph that survives light and dark menu bars.
fn build_icon(status: ResidentStatus) -> anyhow::Result<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(build_icon_rgba(status), 32, 32)
        .map_err(|error| anyhow::anyhow!("building tray icon: {error}"))
}

fn build_icon_rgba(status: ResidentStatus) -> Vec<u8> {
    const SIZE: usize = 32;
    let mut rgba = vec![0; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let body = ((7..=24).contains(&x) && matches!(y, 8 | 9 | 26 | 27))
                || ((8..=27).contains(&y) && matches!(x, 7 | 8 | 23 | 24));
            let clip = ((12..=19).contains(&x) && matches!(y, 4 | 5 | 9 | 10))
                || ((4..=10).contains(&y) && matches!(x, 12 | 13 | 18 | 19));
            let check = matches!(status, ResidentStatus::Active | ResidentStatus::PasteSent)
                && (near_segment(x, y, (11, 17), (14, 20), 1.35)
                    || near_segment(x, y, (14, 20), (21, 13), 1.35));
            let paused = status == ResidentStatus::Paused
                && ((12..=14).contains(&x) || (18..=20).contains(&x))
                && (14..=21).contains(&y);
            let degraded = status == ResidentStatus::Degraded
                && (((15..=17).contains(&x) && (12..=19).contains(&y))
                    || ((15..=17).contains(&x) && (22..=24).contains(&y)));
            let locked = status == ResidentStatus::Locked
                && ((((12..=20).contains(&x)) && matches!(y, 17 | 24))
                    || ((17..=24).contains(&y) && matches!(x, 12 | 20))
                    || (near_segment(x, y, (14, 17), (14, 13), 1.2)
                        || near_segment(x, y, (18, 13), (18, 17), 1.2)
                        || near_segment(x, y, (14, 13), (18, 13), 1.2)));

            let color = if check {
                if status == ResidentStatus::PasteSent {
                    [0xff, 0xc8, 0x4a, 0xff]
                } else {
                    [0x42, 0xd3, 0xb2, 0xff]
                }
            } else if paused {
                [0xe0, 0xa2, 0x32, 0xff]
            } else if degraded {
                [0xd4, 0x4f, 0x59, 0xff]
            } else if locked {
                [0x92, 0xa0, 0xb2, 0xff]
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
        let rgba = build_icon_rgba(ResidentStatus::Active);
        let opaque = rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();

        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!((100..300).contains(&opaque));
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn status_variants_change_pixels_without_changing_dimensions() {
        let active = build_icon_rgba(ResidentStatus::Active);
        for status in [
            ResidentStatus::Paused,
            ResidentStatus::Degraded,
            ResidentStatus::Locked,
            ResidentStatus::PasteSent,
        ] {
            let variant = build_icon_rgba(status);
            assert_eq!(variant.len(), active.len());
            assert_ne!(variant, active);
        }
    }

    #[test]
    fn paused_status_overrides_underlying_capture_health() {
        assert_eq!(
            capture_status_text(true, CaptureHealth::StorageError),
            "Capture paused"
        );
        assert_eq!(
            capture_status_text(false, CaptureHealth::StorageError),
            "History write issue"
        );
    }
}
