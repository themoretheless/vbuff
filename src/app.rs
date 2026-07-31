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
use vbuff_core::onboarding::DefaultProfile;
use vbuff_core::workflow::plain_text_clone;
use vbuff_gui::{DeliveryCapabilities, PopupApp, SharedState, UiAction};
use vbuff_platform::lifecycle::SessionContext;
use vbuff_platform::{GlobalHotkeyBackend, HotkeyBackend, PastePermissionLevel};
use vbuff_types::{
    CapabilityView, CapabilityViewLevel, CapabilityViewSeverity, CapturePauseReason, ClientIntent,
    NoticeLevel,
};

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
    /// The process session snapshot taken once in `main`; the event loop
    /// passes it on rather than detecting the session a second time.
    pub(crate) session: &'static SessionContext,
    pub(crate) strict_capture_blocked: bool,
    pub(crate) automatic_pause_reason: Option<CapturePauseReason>,
    pub(crate) hotkey_registered: bool,
}

pub(crate) fn run(
    services: AppServices,
    mut hotkey_backend: Option<GlobalHotkeyBackend>,
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
        hotkey_id,
        #[cfg(feature = "tray")]
        menu_events,
    );
    let result = eframe::run_ui_native("vbuff", native_options, move |ui, frame| {
        runtime.update(ui, frame);
    });

    if let (Some(id), Some(backend)) = (hotkey_id, hotkey_backend.as_mut())
        && let Err(error) = backend.unregister(id)
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
    hotkey_id: Option<u32>,
    event_waker: Arc<Mutex<Option<egui::Context>>>,
    paused: Arc<AtomicBool>,
    strict_capture_blocked: bool,
    automatic_pause_reason: Option<CapturePauseReason>,
    config: Config,
    popup: PopupApp,
    paste: PasteCoordinator,
    quit_requested: bool,
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
        hotkey_id: Option<u32>,
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
            session,
            strict_capture_blocked,
            automatic_pause_reason,
            hotkey_registered,
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
            state.hotkey_label = hotkey_registered.then(|| config.hotkey.clone());
            state.show_hotkey_coachmark = hotkey_registered && !config.hotkey_coachmark_seen;
            state
                .capabilities
                .retain(|capability| capability.feature != "global_hotkey");
            state.capabilities.push(CapabilityView {
                feature: "global_hotkey".into(),
                level: if hotkey_registered {
                    CapabilityViewLevel::Active
                } else {
                    CapabilityViewLevel::Unavailable
                },
                detail: if hotkey_registered {
                    "configured summon shortcut is registered".into()
                } else {
                    "summon shortcut is unavailable; use the visible window, tray, or relaunch"
                        .into()
                },
                severity: CapabilityViewSeverity::Informational,
            });
        }

        let paste = PasteCoordinator::system(self_writes, session);
        let permission = paste.permission_check();
        if let Ok(mut state) = shared.lock() {
            state
                .capabilities
                .retain(|capability| capability.feature != "paste_permission");
            state.capabilities.push(CapabilityView {
                feature: "paste_permission".into(),
                level: match permission.level {
                    PastePermissionLevel::Automatic => CapabilityViewLevel::Active,
                    PastePermissionLevel::CopyOnly => CapabilityViewLevel::Unavailable,
                },
                detail: permission.detail.into(),
                severity: CapabilityViewSeverity::Informational,
            });
        }

        // Builds without the tray feature have no tray attempt later, so
        // publish the capability (and the matching warning) once up front.
        #[cfg(not(feature = "tray"))]
        {
            Self::publish_tray_capability(
                &shared,
                CapabilityViewLevel::Unavailable,
                "tray icon is unavailable in this build; summon with the hotkey or by relaunching, exit from the popup menu",
            );
            diagnostics.notice(
                NoticeLevel::Warning,
                "Tray icon unavailable; vbuff keeps running — summon with the hotkey or by relaunching, exit from the popup menu",
            );
        }

        let mut popup = PopupApp::new(Arc::clone(&shared));
        // The GUI has no platform dependency of its own, so it is told what
        // session it is running in instead of reading the environment behind
        // the platform layer's back.
        popup.set_session_environment(session.environment_label());
        popup.set_preferences(config.ui_preferences());
        popup.set_delivery_capabilities(DeliveryCapabilities {
            automatic_paste: permission.level == PastePermissionLevel::Automatic,
            sensitive_copy: false,
        });

        Self {
            history,
            popup,
            shared,
            diagnostics,
            instance_intents,
            hotkey_events,
            hotkey_id,
            event_waker,
            paused,
            strict_capture_blocked,
            automatic_pause_reason,
            config,
            paste,
            quit_requested: false,
            #[cfg(feature = "tray")]
            tray: None,
            #[cfg(feature = "tray")]
            menu_events,
            #[cfg(feature = "tray")]
            tray_attempted: false,
        }
    }

    fn update(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Ok(mut target) = self.event_waker.lock() {
            *target = Some(ctx.clone());
        }
        self.ensure_tray();
        if ctx.input(|input| input.viewport().close_requested()) {
            match close_disposition(self.quit_requested) {
                CloseDisposition::CancelAndHide => {
                    // Hide never terminates the resident process: OS close
                    // requests (Esc, close button, focus loss) only hide the
                    // popup, so capture keeps running and the window can be
                    // summoned again via the global hotkey or a relaunch.
                    // Without this CancelClose the process would die
                    // indistinguishably from a hide whenever no tray icon is
                    // available.
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.popup.request_hide(&ctx);
                }
                // Only an explicit Quit sets quit_requested; it is the sole
                // path allowed past CancelClose.
                CloseDisposition::AllowClose => {}
            }
        }
        ctx.request_repaint_after(SUPERVISORY_REPAINT_INTERVAL);

        while let Ok(intent) = self.instance_intents.try_recv() {
            match intent {
                ClientIntent::ShowPopup => self.handle(AppCommand::Show, &ctx),
                ClientIntent::Ping => {}
            }
        }

        while let Ok(event) = self.hotkey_events.try_recv() {
            if event.state == global_hotkey::HotKeyState::Pressed
                && self.hotkey_id == Some(event.id)
            {
                self.handle(AppCommand::Show, &ctx);
            }
        }

        for command in self.tray_commands() {
            self.handle(command, &ctx);
        }

        self.popup.ui(ui, frame);
        let popup_commands: Vec<AppCommand> = self
            .popup
            .take_actions()
            .into_iter()
            .map(AppCommand::from)
            .collect();
        for command in popup_commands {
            self.handle(command, &ctx);
        }

        self.poll_pending_paste(&ctx);
    }

    #[cfg(feature = "tray")]
    fn ensure_tray(&mut self) {
        if self.tray_attempted {
            return;
        }
        self.tray_attempted = true;
        match Tray::new() {
            Ok(tray) => {
                #[cfg(target_os = "linux")]
                tracing::info!(
                    fallback = ?vbuff_platform::LinuxTrayFallback::choose(true, false),
                    "Linux resident surface selected"
                );
                self.tray = Some(tray);
                Self::publish_tray_capability(
                    &self.shared,
                    CapabilityViewLevel::Active,
                    "tray icon is available",
                );
            }
            Err(error) => {
                #[cfg(target_os = "linux")]
                tracing::warn!(
                    fallback = ?vbuff_platform::LinuxTrayFallback::choose(false, false),
                    "tray icon unavailable: {error}; launching vbuff again opens the resident popup"
                );
                #[cfg(not(target_os = "linux"))]
                tracing::warn!("tray icon unavailable: {error}");
                Self::publish_tray_capability(
                    &self.shared,
                    CapabilityViewLevel::Unavailable,
                    "tray icon is unavailable; summon with the hotkey or by relaunching, exit from the popup menu",
                );
                self.notice(
                    NoticeLevel::Warning,
                    "Tray icon unavailable; vbuff keeps running — summon with the hotkey or by relaunching, exit from the popup menu",
                );
            }
        }
    }

    #[cfg(not(feature = "tray"))]
    fn ensure_tray(&mut self) {}

    /// Publish the tray-icon capability exactly once, replacing any prior
    /// entry so repeated calls never duplicate the feature row.
    fn publish_tray_capability(
        shared: &SharedState,
        level: CapabilityViewLevel,
        detail: impl Into<String>,
    ) {
        if let Ok(mut state) = shared.lock() {
            state
                .capabilities
                .retain(|capability| capability.feature != "tray_icon");
            state.capabilities.push(CapabilityView {
                feature: "tray_icon".into(),
                level,
                detail: detail.into(),
                severity: CapabilityViewSeverity::Informational,
            });
        }
    }

    #[cfg(feature = "tray")]
    fn tray_commands(&self) -> Vec<AppCommand> {
        let Some(tray) = &self.tray else {
            return Vec::new();
        };
        if let Ok(state) = self.shared.lock() {
            tray.sync_state(
                state.paused,
                state.pause_reason,
                state.capture_health,
                state.clips.len(),
                self.config.launch_at_login,
                Instant::now(),
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
            AppCommand::Ui(action) => self.handle_ui_action(action, ctx),
            AppCommand::Show => {
                if let Ok(mut state) = self.shared.lock() {
                    state.request_show();
                }
            }
            #[cfg(feature = "tray")]
            AppCommand::CopyLatest => match self.history.latest() {
                Ok(Some(clip)) => match self.paste.copy(&clip.flavors, clip.meta.sensitive) {
                    Ok(()) => self.notice(NoticeLevel::Info, "Latest clip copied"),
                    Err(error) => {
                        let message = if clip.meta.sensitive {
                            "Sensitive clip was not copied because OS history exclusion is unavailable"
                        } else {
                            "Couldn't copy the latest clip"
                        };
                        self.notice(NoticeLevel::Error, message);
                        self.announce(message);
                        tracing::warn!("copy latest failed: {error}");
                    }
                },
                Ok(None) => self.notice(NoticeLevel::Warning, "Clipboard history is empty"),
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't read clipboard history");
                    tracing::warn!("reading latest clip failed: {error}");
                }
            },
            #[cfg(feature = "tray")]
            AppCommand::RequestClearHistory => {
                self.popup.request_clear_history_confirmation(ctx);
            }
            #[cfg(feature = "tray")]
            AppCommand::ToggleAutostart => self.toggle_autostart(),
        }
    }

    fn handle_ui_action(&mut self, action: UiAction, ctx: &egui::Context) {
        match action {
            UiAction::Paste(id) => self.start_paste(id, ctx),
            UiAction::PasteText { text, sensitive } => {
                let text = text.into_string();
                let sensitive = edited_text_requires_sensitive_write(&text, sensitive);
                let flavors = [vbuff_types::Flavor::inline(
                    "text/plain;charset=utf-8",
                    text.into_bytes(),
                )];
                self.start_paste_flavors(&flavors, sensitive, ctx);
            }
            UiAction::SetPinned(id, pinned) => {
                if let Err(error) = self.history.set_pinned(id, pinned) {
                    self.notice(NoticeLevel::Error, "Couldn't update the pinned state");
                    tracing::warn!("updating pin failed: {error}");
                }
            }
            UiAction::SetSessionProtected(id, protected) => {
                if let Err(error) = self.history.set_session_protected(id, protected) {
                    self.notice(
                        NoticeLevel::Error,
                        "Couldn't update the capacity-cleanup exception",
                    );
                    tracing::warn!("updating capacity-cleanup exception failed: {error}");
                } else {
                    self.notice(
                        NoticeLevel::Info,
                        if protected {
                            "Kept from capacity cleanup until vbuff exits; expiry and manual deletion still apply"
                        } else {
                            "Capacity-cleanup exception removed"
                        },
                    );
                }
            }
            UiAction::CreatePlainTextClone(id) => {
                let memory_only = self.history.is_memory_only(id);
                let result = self
                    .history
                    .find(id)
                    .and_then(|source| {
                        source
                            .and_then(|clip| plain_text_clone(&clip, chrono::Utc::now()))
                            .ok_or_else(|| anyhow::anyhow!("clip has no realized text flavor"))
                    })
                    .and_then(|clone| {
                        if memory_only? {
                            self.history.insert_volatile(clone)
                        } else {
                            self.history.insert(&clone, self.config.max_history)
                        }
                    });
                match result {
                    Ok(()) => self.notice(NoticeLevel::Info, "Plain-text clone created"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't create a plain-text clone");
                        tracing::warn!("creating plain-text clone failed: {error}");
                    }
                }
            }
            UiAction::Delete(id) => match self.history.delete(id) {
                Ok(()) => self.notice(NoticeLevel::Info, "Clip deleted"),
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't delete the clip");
                    tracing::warn!("deleting clip failed: {error}");
                }
            },
            UiAction::RestoreClip(clip) => {
                match self
                    .history
                    .restore(clip.into_clip(), self.config.max_history)
                {
                    Ok(()) => self.notice(NoticeLevel::Info, "Clip restored"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't restore the clip");
                        tracing::warn!("restoring deleted clip failed: {error}");
                    }
                }
            }
            UiAction::ClearHistory => match self.history.clear_history() {
                Ok(()) => {
                    self.notice(
                        NoticeLevel::Info,
                        "History cleared; pinned clips and capacity-cleanup exceptions kept",
                    );
                }
                Err(error) => {
                    self.notice(NoticeLevel::Error, "Couldn't clear clipboard history");
                    tracing::warn!("clearing history failed: {error}");
                }
            },
            UiAction::TogglePause => self.toggle_pause(),
            UiAction::RecoverSkipped => {
                if self.diagnostics.request_skipped_recovery() {
                    self.notice(NoticeLevel::Info, "Keeping the current clipboard locally");
                } else {
                    self.notice(
                        NoticeLevel::Warning,
                        "The skipped clipboard is no longer current",
                    );
                }
            }
            UiAction::InstallStarterPack(pack) => {
                let clips = crate::seed_pack::clips(pack);
                match self.history.insert_many(&clips, self.config.max_history) {
                    Ok(()) => self.notice(NoticeLevel::Info, "Starter examples added locally"),
                    Err(error) => {
                        self.notice(NoticeLevel::Error, "Couldn't add starter examples");
                        tracing::warn!("installing starter pack failed: {error}");
                    }
                }
            }
            UiAction::ApplyDefaultProfile(profile) => self.apply_default_profile(profile),
            UiAction::SetLaunchAtLogin(enabled) => self.set_autostart(enabled),
            UiAction::SetUiPreferences {
                preferences,
                reduced_motion_changed,
            } => {
                let previous = self.config.clone();
                self.config
                    .apply_ui_preferences(&preferences, reduced_motion_changed);
                if let Err(error) = self.config.save() {
                    self.config = previous;
                    self.popup.set_preferences(self.config.ui_preferences());
                    self.notice(NoticeLevel::Error, "Couldn't save interface settings");
                    tracing::warn!("saving interface settings failed: {error}");
                }
            }
            UiAction::DismissHealthAlert => {
                if let Ok(mut state) = self.shared.lock() {
                    state.health_alert = None;
                }
            }
            UiAction::DismissSizeBudgetAlert => {
                if let Ok(mut state) = self.shared.lock() {
                    state.size_budget_alert = None;
                }
            }
            UiAction::DismissNotice => self.clear_notice(),
            UiAction::DismissHotkeyCoachmark => {
                self.config.hotkey_coachmark_seen = true;
                if let Ok(mut state) = self.shared.lock() {
                    state.show_hotkey_coachmark = false;
                }
                if let Err(error) = self.config.save() {
                    tracing::warn!("saving hotkey coachmark state failed: {error}");
                }
            }
            // Hide only ever hides the popup; the process exits solely via
            // an explicit Quit.
            UiAction::Hide => self.popup.request_hide(ctx),
            UiAction::Quit => {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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

        self.start_paste_flavors(&clip.flavors, clip.meta.sensitive, ctx);
    }

    fn start_paste_flavors(
        &mut self,
        flavors: &[vbuff_types::Flavor],
        sensitive: bool,
        ctx: &egui::Context,
    ) {
        match self.paste.schedule(flavors, sensitive, Instant::now()) {
            Ok(outcome) => {
                if outcome == PasteOutcome::CopiedOnly {
                    self.announce("Copied. Paste manually.");
                    tracing::info!("clip copied; automatic paste is unavailable");
                }
                // Both outcomes only hide the popup (request_hide issues the
                // visibility command itself); hiding never ends the process.
                self.clear_notice();
                self.popup.request_hide(ctx);
                ctx.request_repaint_after(PASTE_REPAINT_INTERVAL);
            }
            Err(error) => {
                // Keep the popup visible: sending a paste after a failed write
                // could paste unrelated clipboard contents into the target app.
                let message = if sensitive {
                    "Not copied. Sensitive clipboard-history protection is unavailable; clipboard unchanged."
                } else {
                    "Copy failed; clipboard unchanged."
                };
                self.notice(NoticeLevel::Error, message);
                self.announce(message);
                tracing::warn!("selected clip was not staged for paste: {error}");
            }
        }
    }

    fn poll_pending_paste(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(result) = self.paste.poll(now) {
            match result {
                Ok(()) => {
                    self.announce("Paste shortcut sent");
                    #[cfg(feature = "tray")]
                    if let Some(tray) = &self.tray {
                        tray.acknowledge_paste(now);
                    }
                    ctx.request_repaint_after(Duration::from_millis(700));
                }
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
                state.pause_reason = Some(CapturePauseReason::SecurityPolicy);
            }
            self.diagnostics.notice(
                NoticeLevel::Warning,
                "Strict security mode still blocks capture",
            );
            return;
        }
        if let Some(reason) = self.automatic_pause_reason {
            self.paused.store(true, Ordering::Relaxed);
            if let Ok(mut state) = self.shared.lock() {
                state.paused = true;
                state.pause_reason = Some(reason);
            }
            self.diagnostics
                .notice(NoticeLevel::Warning, reason.label());
            return;
        }
        let paused = !self.paused.load(Ordering::Relaxed);
        self.paused.store(paused, Ordering::Relaxed);
        if let Ok(mut state) = self.shared.lock() {
            state.paused = paused;
            state.pause_reason = paused.then_some(CapturePauseReason::Manual);
        }
        tracing::info!(paused, "capture pause toggled");
    }

    fn apply_default_profile(&mut self, profile: DefaultProfile) {
        let previous = self.config.clone();
        self.config.apply_default_profile(profile);
        if let Err(error) = self.config.save() {
            self.config = previous;
            self.notice(
                NoticeLevel::Error,
                "Couldn't save profile; preview config migration first",
            );
            tracing::warn!("saving default profile failed: {error}");
            return;
        }
        if let Ok(mut state) = self.shared.lock() {
            state.default_profile = Some(profile);
        }
        self.notice(
            NoticeLevel::Info,
            "Profile saved; restart vbuff to apply capture and auto-pause policy",
        );
    }

    #[cfg(feature = "tray")]
    fn toggle_autostart(&mut self) {
        let desired = !self.config.launch_at_login;
        self.set_autostart(desired);
    }

    fn set_autostart(&mut self, desired: bool) {
        if desired == self.config.launch_at_login {
            return;
        }
        let previous = self.config.clone();
        self.config.launch_at_login = desired;
        if let Err(error) = self.config.save() {
            self.config = previous;
            self.notice(
                NoticeLevel::Error,
                "Couldn't save login startup; nothing changed",
            );
            tracing::warn!("saving launch-at-login config failed: {error}");
            return;
        }
        if let Err(error) = autostart::set_enabled(desired) {
            self.config = previous;
            if let Err(rollback_error) = self.config.save() {
                tracing::error!(
                    "restoring launch-at-login config after OS failure failed: {rollback_error}"
                );
            }
            self.notice(NoticeLevel::Error, "Couldn't change login startup");
            tracing::warn!("launch-at-login toggle failed: {error}");
            return;
        }
        if let Ok(mut state) = self.shared.lock() {
            state.launch_at_login = desired;
        }
        self.notice(
            NoticeLevel::Info,
            if desired {
                "Start at login enabled"
            } else {
                "Start at login disabled"
            },
        );
        tracing::info!(launch_at_login = desired, "launch-at-login changed");
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

fn edited_text_requires_sensitive_write(text: &str, inherited: bool) -> bool {
    inherited || vbuff_core::capture::text_requires_sensitive_handling(text)
}

/// What the event loop does with an OS close request on the popup viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseDisposition {
    /// Cancel the OS close and hide the popup instead; the resident process
    /// keeps running.
    CancelAndHide,
    /// Allow the close to proceed; only an explicit Quit reaches this.
    AllowClose,
}

/// Close-request policy: hiding never terminates the resident process, so the
/// decision depends only on whether an explicit Quit was requested — never on
/// whether a tray icon (or any other resident surface) exists.
fn close_disposition(quit_requested: bool) -> CloseDisposition {
    if quit_requested {
        CloseDisposition::AllowClose
    } else {
        CloseDisposition::CancelAndHide
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CloseDisposition, Runtime, close_disposition, edited_text_requires_sensitive_write,
    };
    use vbuff_gui::SharedState;
    use vbuff_types::CapabilityViewLevel;

    #[test]
    fn edited_text_preserves_or_redetects_sensitivity() {
        assert!(edited_text_requires_sensitive_write("ordinary", true));
        assert!(edited_text_requires_sensitive_write(
            "ghp_abcdefghijklmnopqrstuvwxyz123456",
            false
        ));
        assert!(edited_text_requires_sensitive_write("123456", false));
        assert!(!edited_text_requires_sensitive_write("ordinary", false));
    }

    #[test]
    fn tray_capability_is_published_exactly_once() {
        let shared = SharedState::default();
        Runtime::publish_tray_capability(&shared, CapabilityViewLevel::Unavailable, "first");
        Runtime::publish_tray_capability(&shared, CapabilityViewLevel::Active, "second");
        let state = shared.lock().expect("shared state lock");
        let entries: Vec<_> = state
            .capabilities
            .iter()
            .filter(|capability| capability.feature == "tray_icon")
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, CapabilityViewLevel::Active);
        assert_eq!(entries[0].detail, "second");
    }

    #[test]
    fn close_disposition_never_depends_on_a_resident_surface() {
        // Hiding never ends the process: without an explicit Quit every close
        // request is cancelled into a hide, regardless of tray availability
        // (the policy function takes no tray input at all).
        assert_eq!(close_disposition(false), CloseDisposition::CancelAndHide);
        assert_eq!(close_disposition(true), CloseDisposition::AllowClose);
    }
}
