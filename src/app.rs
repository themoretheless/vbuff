//! eframe event-loop coordination.
//!
//! This module translates high-level commands into side effects. Capture,
//! persistence, paste timing, tray rendering, and popup rendering live in their
//! own modules.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::App as _;
use global_hotkey::GlobalHotKeyEvent;
use vbuff_core::capture::SelfWriteLedger;
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
#[cfg(feature = "tray")]
use tray_icon::menu::MenuEvent;

const SUPERVISORY_REPAINT_INTERVAL: Duration = Duration::from_secs(5);
const PASTE_REPAINT_INTERVAL: Duration = Duration::from_millis(20);

/// Resident services consumed by the eframe event loop.
pub(crate) struct AppServices {
    pub(crate) history: History,
    pub(crate) shared: SharedState,
    pub(crate) diagnostics: Diagnostics,
    pub(crate) instance_intents: Receiver<ClientIntent>,
    pub(crate) paused: Arc<AtomicBool>,
    pub(crate) config: Config,
    pub(crate) self_writes: Arc<std::sync::Mutex<SelfWriteLedger>>,
    pub(crate) strict_capture_blocked: bool,
}

pub(crate) fn run(
    services: AppServices,
    mut hotkey_backend: GlobalHotkeyBackend,
    hotkey_id: Option<u32>,
) -> anyhow::Result<()> {
    let event_waker = Arc::new(Mutex::new(None::<egui::Context>));
    let (hotkey_sender, hotkey_events) = channel();
    let hotkey_waker = Arc::clone(&event_waker);
    GlobalHotKeyEvent::set_event_handler(Some(move |event| {
        let _ = hotkey_sender.send(event);
        request_event_repaint(&hotkey_waker);
    }));
    #[cfg(feature = "tray")]
    let menu_events = {
        let (menu_sender, menu_events) = channel();
        let menu_waker = Arc::clone(&event_waker);
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_sender.send(event);
            request_event_repaint(&menu_waker);
        }));
        menu_events
    };

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

    let mut runtime = Runtime::new(
        services,
        event_waker,
        hotkey_events,
        #[cfg(feature = "tray")]
        menu_events,
    );
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

fn request_event_repaint(target: &Arc<Mutex<Option<egui::Context>>>) {
    if let Ok(target) = target.lock()
        && let Some(ctx) = target.as_ref()
    {
        ctx.request_repaint();
    }
}

struct Runtime {
    history: History,
    shared: SharedState,
    diagnostics: Diagnostics,
    instance_intents: Receiver<ClientIntent>,
    hotkey_events: Receiver<GlobalHotKeyEvent>,
    event_waker: Arc<Mutex<Option<egui::Context>>>,
    paused: Arc<AtomicBool>,
    strict_capture_blocked: bool,
    config: Config,
    popup: PopupApp,
    paste: PasteCoordinator,
    #[cfg(feature = "tray")]
    tray: Option<Tray>,
    #[cfg(feature = "tray")]
    menu_events: Receiver<MenuEvent>,
    #[cfg(feature = "tray")]
    tray_attempted: bool,
}

impl Runtime {
    fn new(
        services: AppServices,
        event_waker: Arc<Mutex<Option<egui::Context>>>,
        hotkey_events: Receiver<GlobalHotKeyEvent>,
        #[cfg(feature = "tray")] menu_events: Receiver<MenuEvent>,
    ) -> Self {
        let AppServices {
            history,
            shared,
            diagnostics,
            instance_intents: upstream_instance_intents,
            paused,
            config,
            self_writes,
            strict_capture_blocked,
        } = services;
        let (intent_sender, instance_intents) = channel();
        let intent_waker = Arc::clone(&event_waker);
        std::thread::spawn(move || {
            while let Ok(intent) = upstream_instance_intents.recv() {
                if intent_sender.send(intent).is_err() {
                    break;
                }
                request_event_repaint(&intent_waker);
            }
        });

        if let Ok(mut state) = shared.lock() {
            state.hotkey_label = Some(config.hotkey.clone());
            state.show_hotkey_coachmark = !config.hotkey_coachmark_seen;
        }

        Self {
            history,
            popup: PopupApp::new(Arc::clone(&shared)),
            shared,
            diagnostics,
            instance_intents,
            hotkey_events,
            event_waker,
            paused,
            strict_capture_blocked,
            config,
            paste: PasteCoordinator::system(self_writes),
            #[cfg(feature = "tray")]
            tray: None,
            #[cfg(feature = "tray")]
            menu_events,
            #[cfg(feature = "tray")]
            tray_attempted: false,
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Ok(mut target) = self.event_waker.lock() {
            *target = Some(ctx.clone());
        }
        self.ensure_tray();
        ctx.request_repaint_after(SUPERVISORY_REPAINT_INTERVAL);

        while let Ok(intent) = self.instance_intents.try_recv() {
            match intent {
                ClientIntent::ShowPopup => self.handle(AppCommand::Show, ctx),
                ClientIntent::Ping => {}
            }
        }

        while let Ok(event) = self.hotkey_events.try_recv() {
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
        let mut commands = Vec::new();
        while let Ok(event) = self.menu_events.try_recv() {
            if let Some(command) = tray.command_for(&event) {
                commands.push(command);
            }
        }
        commands
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
            AppCommand::PasteText(text) => {
                let flavors = [vbuff_types::Flavor::inline(
                    "text/plain;charset=utf-8",
                    text.into_bytes(),
                )];
                self.start_paste_flavors(&flavors, ctx);
            }
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
            AppCommand::RestoreClip(clip) => {
                match self.history.insert(&clip, self.config.max_history) {
                    Ok(()) => self.notice(NoticeLevel::Info, "Clip restored"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't restore the clip");
                        tracing::warn!("restoring deleted clip failed: {error}");
                    }
                }
            }
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
            AppCommand::RecoverSkipped => {
                if self.diagnostics.request_skipped_recovery() {
                    self.notice(NoticeLevel::Info, "Keeping the current clipboard locally");
                } else {
                    self.notice(
                        NoticeLevel::Warning,
                        "The skipped clipboard is no longer current",
                    );
                }
            }
            AppCommand::InstallStarterPack(pack) => {
                let clips = crate::seed_pack::clips(pack);
                match self.history.insert_many(&clips, self.config.max_history) {
                    Ok(()) => self.notice(NoticeLevel::Info, "Starter examples added locally"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't add starter examples");
                        tracing::warn!("installing starter pack failed: {error}");
                    }
                }
            }
            #[cfg(feature = "tray")]
            AppCommand::ToggleAutostart => self.toggle_autostart(),
            AppCommand::DismissNotice => self.clear_notice(),
            AppCommand::DismissHotkeyCoachmark => {
                self.config.hotkey_coachmark_seen = true;
                if let Ok(mut state) = self.shared.lock() {
                    state.show_hotkey_coachmark = false;
                }
                if let Err(error) = self.config.save() {
                    tracing::warn!("saving hotkey coachmark state failed: {error}");
                }
            }
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

        self.start_paste_flavors(&clip.flavors, ctx);
    }

    fn start_paste_flavors(&mut self, flavors: &[vbuff_types::Flavor], ctx: &egui::Context) {
        match self.paste.schedule(flavors, Instant::now()) {
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
        if let Some(result) = self.paste.poll(now) {
            match result {
                Ok(()) => self.announce("Paste complete"),
                Err(error) => {
                    self.notice(
                        NoticeLevel::Error,
                        "Automatic paste failed; the clip remains copied",
                    );
                    self.announce("Automatic paste failed; clip remains copied");
                    tracing::warn!("paste-back failed: {error}");
                }
            }
        }
        if self.paste.wait_duration(now).is_some() {
            ctx.request_repaint_after(PASTE_REPAINT_INTERVAL);
        }
    }

    fn toggle_pause(&self) {
        if self.strict_capture_blocked {
            self.paused.store(true, Ordering::Relaxed);
            if let Ok(mut state) = self.shared.lock() {
                state.paused = true;
            }
            self.diagnostics.notice(
                NoticeLevel::Warning,
                "Strict security mode still blocks capture",
            );
            return;
        }
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

    fn announce(&self, message: &'static str) {
        if let Ok(mut state) = self.shared.lock() {
            state.announce(message);
        }
    }
}
