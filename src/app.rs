//! eframe event-loop coordination.
//!
//! This module translates high-level commands into side effects. Capture,
//! persistence, paste timing, tray rendering, and popup rendering live in their
//! own modules.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::App as _;
use global_hotkey::GlobalHotKeyEvent;
use vbuff_gui::{PopupApp, SharedState};
use vbuff_platform::{GlobalHotkeyBackend, HotkeyBackend};
use vbuff_types::{ClientIntent, NoticeLevel};

#[cfg(feature = "tray")]
use crate::autostart;
use crate::commands::AppCommand;
use crate::config::Config;
use crate::diagnostics::Diagnostics;
use crate::history::History;
use crate::paste::{PasteCoordinator, PasteOutcome};
#[cfg(feature = "tray")]
use crate::tray::Tray;

const IDLE_REPAINT_INTERVAL: Duration = Duration::from_millis(100);
const PASTE_REPAINT_INTERVAL: Duration = Duration::from_millis(20);

/// Resident services consumed by the eframe event loop.
pub(crate) struct AppServices {
    history: History,
    shared: SharedState,
    diagnostics: Diagnostics,
    instance_intents: Receiver<ClientIntent>,
    paused: Arc<AtomicBool>,
    config: Config,
}

impl AppServices {
    pub(crate) fn new(
        history: History,
        shared: SharedState,
        diagnostics: Diagnostics,
        instance_intents: Receiver<ClientIntent>,
        paused: Arc<AtomicBool>,
        config: Config,
    ) -> Self {
        Self {
            history,
            shared,
            diagnostics,
            instance_intents,
            paused,
            config,
        }
    }
}

pub(crate) fn run(
    services: AppServices,
    mut hotkey_backend: GlobalHotkeyBackend,
    hotkey_id: Option<u32>,
) -> anyhow::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("vbuff")
        .with_inner_size(vbuff_gui::popup_size())
        .with_min_inner_size(vbuff_gui::popup_min_size())
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_visible(false);
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let mut runtime = Runtime::new(services);
    let result = eframe::run_simple_native("vbuff", native_options, move |ctx, frame| {
        runtime.update(ctx, frame);
    });

    if let Some(id) = hotkey_id
        && let Err(error) = hotkey_backend.unregister(id)
    {
        tracing::warn!("hotkey cleanup failed: {error}");
    }

    result.map_err(|error| anyhow::anyhow!("eframe error: {error}"))
}

struct Runtime {
    history: History,
    shared: SharedState,
    diagnostics: Diagnostics,
    instance_intents: Receiver<ClientIntent>,
    paused: Arc<AtomicBool>,
    #[cfg(feature = "tray")]
    config: Config,
    popup: PopupApp,
    paste: PasteCoordinator,
    ticker_started: bool,
    #[cfg(feature = "tray")]
    tray: Option<Tray>,
    #[cfg(feature = "tray")]
    tray_attempted: bool,
}

impl Runtime {
    fn new(services: AppServices) -> Self {
        let AppServices {
            history,
            shared,
            diagnostics,
            instance_intents,
            paused,
            config,
        } = services;
        #[cfg(not(feature = "tray"))]
        let _ = config;

        Self {
            history,
            popup: PopupApp::new(Arc::clone(&shared)),
            shared,
            diagnostics,
            instance_intents,
            paused,
            #[cfg(feature = "tray")]
            config,
            paste: PasteCoordinator::system(),
            ticker_started: false,
            #[cfg(feature = "tray")]
            tray: None,
            #[cfg(feature = "tray")]
            tray_attempted: false,
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.ensure_ticker(ctx);
        self.ensure_tray();
        ctx.request_repaint_after(IDLE_REPAINT_INTERVAL);

        while let Ok(intent) = self.instance_intents.try_recv() {
            match intent {
                ClientIntent::ShowPopup => self.handle(AppCommand::Show, ctx),
                ClientIntent::Ping => {}
            }
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == global_hotkey::HotKeyState::Pressed {
                self.handle(AppCommand::Show, ctx);
            }
        }

        for command in self.tray_commands() {
            self.handle(command, ctx);
        }

        self.popup.update(ctx, frame);
        let popup_commands: Vec<AppCommand> = self
            .popup
            .take_actions()
            .into_iter()
            .map(AppCommand::from)
            .collect();
        for command in popup_commands {
            self.handle(command, ctx);
        }

        self.poll_pending_paste(ctx);
    }

    fn ensure_ticker(&mut self, ctx: &egui::Context) {
        if self.ticker_started {
            return;
        }
        self.ticker_started = true;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(IDLE_REPAINT_INTERVAL);
                ctx.request_repaint();
            }
        });
    }

    #[cfg(feature = "tray")]
    fn ensure_tray(&mut self) {
        if self.tray_attempted {
            return;
        }
        self.tray_attempted = true;
        match Tray::new() {
            Ok(tray) => self.tray = Some(tray),
            Err(error) => tracing::warn!("tray icon unavailable: {error}"),
        }
    }

    #[cfg(not(feature = "tray"))]
    fn ensure_tray(&mut self) {}

    #[cfg(feature = "tray")]
    fn tray_commands(&self) -> Vec<AppCommand> {
        let Some(tray) = &self.tray else {
            return Vec::new();
        };
        if let Ok(state) = self.shared.lock() {
            tray.sync_state(
                state.paused,
                state.capture_health,
                state.clips.len(),
                self.config.launch_at_login,
            );
        }
        tray.poll()
    }

    #[cfg(not(feature = "tray"))]
    fn tray_commands(&self) -> Vec<AppCommand> {
        Vec::new()
    }

    fn handle(&mut self, command: AppCommand, ctx: &egui::Context) {
        match command {
            AppCommand::Show => {
                if let Ok(mut state) = self.shared.lock() {
                    state.request_show();
                }
            }
            AppCommand::Paste(id) => self.start_paste(id, ctx),
            #[cfg(feature = "tray")]
            AppCommand::CopyLatest => match self.history.latest() {
                Ok(Some(clip)) => match self.paste.copy(&clip.flavors) {
                    Ok(()) => self.notice(NoticeLevel::Info, "Latest clip copied"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't copy the latest clip");
                        tracing::warn!("copy latest failed: {error}");
                    }
                },
                Ok(None) => self.notice(NoticeLevel::Warning, "Clipboard history is empty"),
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't read clipboard history");
                    tracing::warn!("reading latest clip failed: {error}");
                }
            },
            AppCommand::SetPinned(id, pinned) => {
                if let Err(error) = self.history.set_pinned(id, pinned) {
                    self.notice(NoticeLevel::Error, "Couldn't update the pinned state");
                    tracing::warn!("updating pin failed: {error}");
                }
            }
            AppCommand::Delete(id) => match self.history.delete(id) {
                Ok(()) => self.notice(NoticeLevel::Info, "Clip deleted"),
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't delete the clip");
                    tracing::warn!("deleting clip failed: {error}");
                }
            },
            #[cfg(feature = "tray")]
            AppCommand::RequestClearHistory => {
                self.popup.request_clear_history_confirmation(ctx);
            }
            AppCommand::ClearHistory => match self.history.clear_history() {
                Ok(()) => {
                    self.notice(NoticeLevel::Info, "History cleared; pinned clips kept");
                }
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't clear clipboard history");
                    tracing::warn!("clearing history failed: {error}");
                }
            },
            AppCommand::TogglePause => self.toggle_pause(),
            #[cfg(feature = "tray")]
            AppCommand::ToggleAutostart => self.toggle_autostart(),
            AppCommand::DismissNotice => self.clear_notice(),
            AppCommand::Hide => {}
            #[cfg(feature = "tray")]
            AppCommand::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn start_paste(&mut self, id: vbuff_types::ClipId, ctx: &egui::Context) {
        let clip = match self.history.find(id) {
            Ok(Some(clip)) => clip,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!("reading selected clip failed: {error}");
                return;
            }
        };

        match self.paste.schedule(&clip.flavors, Instant::now()) {
            Ok(outcome) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                if outcome == PasteOutcome::CopiedOnly {
                    self.notice(
                        NoticeLevel::Warning,
                        "Automatic paste unavailable; clip copied",
                    );
                    tracing::warn!("clip copied; automatic paste is unavailable");
                } else {
                    self.clear_notice();
                }
                ctx.request_repaint_after(PASTE_REPAINT_INTERVAL);
            }
            Err(error) => {
                // Keep the popup visible: sending a paste after a failed write
                // could paste unrelated clipboard contents into the target app.
                self.notice(NoticeLevel::Error, "Couldn't stage the selected clip");
                tracing::warn!("selected clip was not staged for paste: {error}");
            }
        }
    }

    fn poll_pending_paste(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(result) = self.paste.poll(now)
            && let Err(error) = result
        {
            self.notice(
                NoticeLevel::Error,
                "Automatic paste failed; the clip remains copied",
            );
            tracing::warn!("paste-back failed: {error}");
        }
        if self.paste.wait_duration(now).is_some() {
            ctx.request_repaint_after(PASTE_REPAINT_INTERVAL);
        }
    }

    fn toggle_pause(&self) {
        let paused = !self.paused.load(Ordering::Relaxed);
        self.paused.store(paused, Ordering::Relaxed);
        if let Ok(mut state) = self.shared.lock() {
            state.paused = paused;
        }
        tracing::info!(paused, "capture pause toggled");
    }

    #[cfg(feature = "tray")]
    fn toggle_autostart(&mut self) {
        let desired = !self.config.launch_at_login;
        match autostart::set_enabled(desired) {
            Ok(()) => {
                self.config.launch_at_login = desired;
                if let Err(error) = self.config.save() {
                    self.notice(
                        NoticeLevel::Warning,
                        "Login startup changed, but the setting wasn't saved",
                    );
                    tracing::warn!("saving launch-at-login config failed: {error}");
                } else {
                    self.notice(
                        NoticeLevel::Info,
                        if desired {
                            "Start at login enabled"
                        } else {
                            "Start at login disabled"
                        },
                    );
                }
                tracing::info!(launch_at_login = desired, "launch-at-login toggled");
            }
            Err(error) => {
                self.notice(NoticeLevel::Error, "Couldn't change login startup");
                tracing::warn!("launch-at-login toggle failed: {error}");
            }
        }
    }

    fn notice(&self, level: NoticeLevel, message: &'static str) {
        self.diagnostics.notice(level, message);
    }

    fn clear_notice(&self) {
        self.diagnostics.clear_notice();
    }
}
