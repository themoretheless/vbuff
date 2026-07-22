//! Application configuration, persisted as TOML.
//!
//! The config lives at `<config_dir>/vbuff/config.toml`. It is loaded at start
//! and created with defaults if missing. Policy (hotkey, intervals, exclusions)
//! lives here, not in the database.

use std::fmt;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use vbuff_core::onboarding::DefaultProfile;
use vbuff_gui::{DensityMode, HandedMode, UiPreferences};

const CONFIG_SCHEMA_VERSION: u16 = 2;
const LEGACY_CONFIG_SCHEMA_VERSION: u16 = 1;
const SHAREABLE_CONFIG_SCHEMA: u16 = 2;
const HANDOFF_CONFIG_SCHEMA: u16 = 2;
const MAX_SHAREABLE_CONFIG_BYTES: usize = 256 * 1024;

/// User-tunable configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Version of the owner-local config representation.
    #[serde(default = "legacy_config_schema")]
    pub schema_version: u16,
    /// Global show/hide hotkey, e.g. `"Cmd+Shift+V"` or `"Ctrl+Shift+V"`.
    pub hotkey: String,
    /// Clipboard poll interval in milliseconds.
    pub poll_interval_ms: u64,
    /// Maximum number of clips to retain (count cap).
    pub max_history: usize,
    /// Paste modifier: `"cmd"` or `"ctrl"`. Empty/auto = OS default.
    pub paste_modifier: String,
    /// Source apps to exclude from capture (matched as a substring of the
    /// source-app identifier). Stub-honored in the MVP.
    pub excluded_apps: Vec<String>,
    /// Ordered source-context rules evaluated before content inspection.
    pub source_rules: Vec<SourceRuleConfig>,
    /// Skip capturing empty/whitespace-only text copies.
    pub skip_whitespace_only: bool,
    /// Classify well-known credentials and tokens as short-lived sensitive clips.
    pub detect_secrets: bool,
    /// Retention window for structurally detected secrets.
    pub secret_ttl_seconds: u64,
    /// Full-payload threshold after which capture sheds to a text preview.
    pub capture_soft_limit_bytes: usize,
    /// Absolute per-capture admission cap.
    pub capture_hard_limit_bytes: usize,
    /// Maximum bytes retained by a shed text preview.
    pub capture_preview_bytes: usize,
    /// Resident-memory level that defers background indexing.
    pub memory_soft_limit_mb: usize,
    /// Resident-memory level that aggressively restricts large captures.
    pub memory_hard_limit_mb: usize,
    /// Refuse capture while any required security capability is unavailable.
    pub strict_security_mode: bool,
    /// Register vbuff to launch when the user logs in.
    pub launch_at_login: bool,
    /// Applied first-run profile, if the user selected one.
    pub default_profile: Option<DefaultProfile>,
    /// Pause after this many idle seconds; zero disables idle auto-pause.
    pub auto_pause_idle_seconds: u64,
    /// Pause when a native session adapter reports a screen lock.
    pub auto_pause_on_lock: bool,
    /// Pause when a remote-control session is detected.
    pub auto_pause_remote: bool,
    /// The summon-shortcut coachmark has been acknowledged on this profile.
    pub hotkey_coachmark_seen: bool,
    /// Native history-row density: auto, compact, or comfortable.
    pub ui_density: String,
    /// Disable non-essential native popup animation.
    pub ui_reduced_motion: Option<bool>,
    /// Show the wide clip preview when the viewport has enough room.
    pub ui_large_preview: bool,
    /// Optional one-handed keyboard layout: off, left, or right.
    pub ui_handed_mode: String,
    /// Show the local frame/scroll diagnostic overlay.
    pub ui_motion_inspector: bool,
    /// Expand the metadata-only clipboard health digest.
    pub ui_show_health_digest: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("schema_version", &self.schema_version)
            .field("hotkey", &self.hotkey)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("max_history", &self.max_history)
            .field("paste_modifier", &self.paste_modifier)
            .field("excluded_app_count", &self.excluded_apps.len())
            .field("source_rule_count", &self.source_rules.len())
            .field("skip_whitespace_only", &self.skip_whitespace_only)
            .field("detect_secrets", &self.detect_secrets)
            .field("secret_ttl_seconds", &self.secret_ttl_seconds)
            .field("capture_soft_limit_bytes", &self.capture_soft_limit_bytes)
            .field("capture_hard_limit_bytes", &self.capture_hard_limit_bytes)
            .field("capture_preview_bytes", &self.capture_preview_bytes)
            .field("memory_soft_limit_mb", &self.memory_soft_limit_mb)
            .field("memory_hard_limit_mb", &self.memory_hard_limit_mb)
            .field("strict_security_mode", &self.strict_security_mode)
            .field("launch_at_login", &self.launch_at_login)
            .field("default_profile", &self.default_profile)
            .field("auto_pause_idle_seconds", &self.auto_pause_idle_seconds)
            .field("auto_pause_on_lock", &self.auto_pause_on_lock)
            .field("auto_pause_remote", &self.auto_pause_remote)
            .field("hotkey_coachmark_seen", &self.hotkey_coachmark_seen)
            .field("ui_density", &self.ui_density)
            .field("ui_reduced_motion", &self.ui_reduced_motion)
            .field("ui_large_preview", &self.ui_large_preview)
            .field("ui_handed_mode", &self.ui_handed_mode)
            .field("ui_motion_inspector", &self.ui_motion_inspector)
            .field("ui_show_health_digest", &self.ui_show_health_digest)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            hotkey: default_hotkey().to_string(),
            poll_interval_ms: 300,
            max_history: 500,
            paste_modifier: String::new(),
            excluded_apps: Vec::new(),
            source_rules: Vec::new(),
            skip_whitespace_only: true,
            detect_secrets: true,
            secret_ttl_seconds: 10 * 60,
            capture_soft_limit_bytes: 16 * 1024 * 1024,
            capture_hard_limit_bytes: 128 * 1024 * 1024,
            capture_preview_bytes: 256 * 1024,
            memory_soft_limit_mb: 512,
            memory_hard_limit_mb: 1_024,
            strict_security_mode: false,
            launch_at_login: false,
            default_profile: None,
            auto_pause_idle_seconds: 15 * 60,
            auto_pause_on_lock: true,
            auto_pause_remote: true,
            hotkey_coachmark_seen: false,
            ui_density: "auto".into(),
            ui_reduced_motion: None,
            ui_large_preview: true,
            ui_handed_mode: "off".into(),
            ui_motion_inspector: false,
            ui_show_health_digest: false,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceRuleConfig {
    pub app_contains: Option<String>,
    pub title_regex: Option<String>,
    pub url_host_suffix: Option<String>,
    pub action: SourceRuleAction,
}

impl fmt::Debug for SourceRuleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceRuleConfig")
            .field("has_app_matcher", &self.app_contains.is_some())
            .field("has_title_matcher", &self.title_regex.is_some())
            .field("has_host_matcher", &self.url_host_suffix.is_some())
            .field("action", &self.action)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRuleAction {
    #[default]
    Capture,
    Skip,
    PlainTextOnly,
    StripImages,
    CaptureSensitive,
}

/// Deliberately excludes app exclusions, source rules, and clipboard data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareableConfig {
    pub schema: u16,
    pub hotkey: String,
    pub poll_interval_ms: u64,
    pub max_history: usize,
    pub paste_modifier: String,
    pub skip_whitespace_only: bool,
    pub detect_secrets: bool,
    pub secret_ttl_seconds: u64,
    pub capture_soft_limit_bytes: usize,
    pub capture_hard_limit_bytes: usize,
    pub capture_preview_bytes: usize,
    pub memory_soft_limit_mb: usize,
    pub memory_hard_limit_mb: usize,
    pub strict_security_mode: bool,
    pub launch_at_login: bool,
    pub default_profile: Option<DefaultProfile>,
    pub auto_pause_idle_seconds: u64,
    pub auto_pause_on_lock: bool,
    pub auto_pause_remote: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigHandoff {
    schema: u16,
    source_platform: String,
    config: Config,
    payload_hash: String,
}

impl fmt::Debug for ConfigHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigHandoff")
            .field("schema", &self.schema)
            .field("source_platform", &self.source_platform)
            .field("config", &self.config)
            .field("payload_hash", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ConfigCommand {
    Export(Option<PathBuf>),
    Apply(PathBuf),
    HandoffExport(PathBuf),
    HandoffApply(PathBuf),
    MigratePreview,
    MigrateApply,
}

impl fmt::Debug for ConfigCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Export(path) => formatter
                .debug_tuple("Export")
                .field(&path.as_ref().map(|_| "[redacted path]"))
                .finish(),
            Self::Apply(_) => formatter.write_str("Apply([redacted path])"),
            Self::HandoffExport(_) => formatter.write_str("HandoffExport([redacted path])"),
            Self::HandoffApply(_) => formatter.write_str("HandoffApply([redacted path])"),
            Self::MigratePreview => formatter.write_str("MigratePreview"),
            Self::MigrateApply => formatter.write_str("MigrateApply"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConfigMigrationPreview {
    from_schema: u16,
    to_schema: u16,
    changes: Vec<&'static str>,
}

const fn legacy_config_schema() -> u16 {
    LEGACY_CONFIG_SCHEMA_VERSION
}

/// The default hotkey string for the current OS.
fn default_hotkey() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Cmd+Shift+V"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Shift+V"
    }
}

impl Config {
    /// Load the config from the default path, creating it with defaults if it
    /// does not yet exist.
    pub fn load_or_create() -> anyhow::Result<Config> {
        let path = config_path()?;
        if path.exists() {
            let text = read_bounded(std::fs::File::open(&path)?)?;
            let cfg = parse_runtime_config(&text)?;
            cfg.validate()?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.validate()?;
            cfg.save()?;
            Ok(cfg)
        }
    }

    /// Load existing policy for read-only diagnostics without creating a file.
    pub fn load_for_inspection() -> anyhow::Result<Config> {
        let path = config_path()?;
        if path.exists() {
            let cfg = parse_runtime_config(&read_bounded(std::fs::File::open(path)?)?)?;
            cfg.validate()?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.validate()?;
            Ok(cfg)
        }
    }

    /// Persist the config to the default path.
    pub fn save(&self) -> anyhow::Result<()> {
        self.validate()?;
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        write_private(&path, &text)?;
        Ok(())
    }

    pub fn shareable(&self) -> ShareableConfig {
        ShareableConfig {
            schema: SHAREABLE_CONFIG_SCHEMA,
            hotkey: self.hotkey.clone(),
            poll_interval_ms: self.poll_interval_ms,
            max_history: self.max_history,
            paste_modifier: self.paste_modifier.clone(),
            skip_whitespace_only: self.skip_whitespace_only,
            detect_secrets: self.detect_secrets,
            secret_ttl_seconds: self.secret_ttl_seconds,
            capture_soft_limit_bytes: self.capture_soft_limit_bytes,
            capture_hard_limit_bytes: self.capture_hard_limit_bytes,
            capture_preview_bytes: self.capture_preview_bytes,
            memory_soft_limit_mb: self.memory_soft_limit_mb,
            memory_hard_limit_mb: self.memory_hard_limit_mb,
            strict_security_mode: self.strict_security_mode,
            launch_at_login: self.launch_at_login,
            default_profile: self.default_profile,
            auto_pause_idle_seconds: self.auto_pause_idle_seconds,
            auto_pause_on_lock: self.auto_pause_on_lock,
            auto_pause_remote: self.auto_pause_remote,
        }
    }

    pub fn apply_shareable(&mut self, shared: ShareableConfig) -> anyhow::Result<()> {
        shared.validate()?;
        self.hotkey = shared.hotkey;
        self.poll_interval_ms = shared.poll_interval_ms;
        self.max_history = shared.max_history;
        self.paste_modifier = shared.paste_modifier;
        self.skip_whitespace_only = shared.skip_whitespace_only;
        self.detect_secrets = shared.detect_secrets;
        self.secret_ttl_seconds = shared.secret_ttl_seconds;
        self.capture_soft_limit_bytes = shared.capture_soft_limit_bytes;
        self.capture_hard_limit_bytes = shared.capture_hard_limit_bytes;
        self.capture_preview_bytes = shared.capture_preview_bytes;
        self.memory_soft_limit_mb = shared.memory_soft_limit_mb;
        self.memory_hard_limit_mb = shared.memory_hard_limit_mb;
        self.strict_security_mode = shared.strict_security_mode;
        self.launch_at_login = shared.launch_at_login;
        self.default_profile = shared.default_profile;
        self.auto_pause_idle_seconds = shared.auto_pause_idle_seconds;
        self.auto_pause_on_lock = shared.auto_pause_on_lock;
        self.auto_pause_remote = shared.auto_pause_remote;
        Ok(())
    }

    pub fn apply_default_profile(&mut self, profile: DefaultProfile) {
        let defaults = profile.defaults();
        self.default_profile = Some(profile);
        self.max_history = defaults.max_history;
        self.secret_ttl_seconds = defaults.secret_ttl_seconds;
        self.capture_soft_limit_bytes = defaults.capture_soft_limit_bytes;
        self.capture_hard_limit_bytes = defaults.capture_hard_limit_bytes;
        self.capture_preview_bytes = self
            .capture_preview_bytes
            .min(defaults.capture_soft_limit_bytes);
        self.auto_pause_idle_seconds = defaults.auto_pause_idle_seconds;
        self.auto_pause_remote = defaults.auto_pause_remote;
        self.detect_secrets = defaults.detect_secrets;
    }

    pub fn ui_preferences(&self) -> UiPreferences {
        UiPreferences {
            density: match self.ui_density.as_str() {
                "compact" => DensityMode::Compact,
                "comfortable" => DensityMode::Comfortable,
                _ => DensityMode::Auto,
            },
            reduced_motion: self
                .ui_reduced_motion
                .or_else(vbuff_platform::desktop::reduced_motion_preference)
                .unwrap_or(false),
            large_preview: self.ui_large_preview,
            handed_mode: match self.ui_handed_mode.as_str() {
                "left" => HandedMode::Left,
                "right" => HandedMode::Right,
                _ => HandedMode::Off,
            },
            motion_inspector: self.ui_motion_inspector,
            show_health_digest: self.ui_show_health_digest,
        }
    }

    pub fn apply_ui_preferences(
        &mut self,
        preferences: &UiPreferences,
        reduced_motion_changed: bool,
    ) {
        self.ui_density = match preferences.density {
            DensityMode::Auto => "auto",
            DensityMode::Compact => "compact",
            DensityMode::Comfortable => "comfortable",
        }
        .into();
        if reduced_motion_changed {
            self.ui_reduced_motion = Some(preferences.reduced_motion);
        }
        self.ui_large_preview = preferences.large_preview;
        self.ui_handed_mode = match preferences.handed_mode {
            HandedMode::Off => "off",
            HandedMode::Left => "left",
            HandedMode::Right => "right",
        }
        .into();
        self.ui_motion_inspector = preferences.motion_inspector;
        self.ui_show_health_digest = preferences.show_health_digest;
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == CONFIG_SCHEMA_VERSION,
            "config schema {} needs previewed migration; run `vbuff config migrate preview`",
            self.schema_version
        );
        self.shareable().validate()?;
        anyhow::ensure!(self.excluded_apps.len() <= 1_024, "too many excluded apps");
        for app in &self.excluded_apps {
            anyhow::ensure!(
                !app.is_empty() && app.len() <= 512 && !app.chars().any(char::is_control),
                "invalid excluded app"
            );
        }
        anyhow::ensure!(self.source_rules.len() <= 1_024, "too many source rules");
        anyhow::ensure!(
            matches!(self.ui_density.as_str(), "auto" | "compact" | "comfortable"),
            "invalid UI density"
        );
        anyhow::ensure!(
            matches!(self.ui_handed_mode.as_str(), "off" | "left" | "right"),
            "invalid handed mode"
        );
        for rule in &self.source_rules {
            for value in [&rule.app_contains, &rule.url_host_suffix]
                .into_iter()
                .flatten()
            {
                anyhow::ensure!(
                    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
                    "invalid source rule"
                );
            }
            if let Some(pattern) = &rule.title_regex {
                anyhow::ensure!(pattern.len() <= 4_096, "source rule regex is too long");
                regex::Regex::new(pattern)
                    .map_err(|_| anyhow::anyhow!("invalid source rule regex"))?;
            }
        }
        Ok(())
    }
}

impl ShareableConfig {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == SHAREABLE_CONFIG_SCHEMA,
            "unsupported config schema"
        );
        anyhow::ensure!(
            !self.hotkey.is_empty()
                && self.hotkey.len() <= 128
                && vbuff_platform::parse_combo(&self.hotkey).is_ok(),
            "invalid hotkey"
        );
        anyhow::ensure!(
            (25..=10_000).contains(&self.poll_interval_ms),
            "invalid poll interval"
        );
        anyhow::ensure!(
            (1..=100_000).contains(&self.max_history),
            "invalid history cap"
        );
        anyhow::ensure!(
            (1..=365 * 24 * 60 * 60).contains(&self.secret_ttl_seconds),
            "invalid secret retention"
        );
        anyhow::ensure!(
            self.capture_preview_bytes > 0
                && self.capture_hard_limit_bytes <= 1024 * 1024 * 1024
                && self.capture_preview_bytes <= self.capture_soft_limit_bytes
                && self.capture_soft_limit_bytes <= self.capture_hard_limit_bytes,
            "invalid capture limits"
        );
        anyhow::ensure!(
            self.memory_soft_limit_mb > 0
                && self.memory_soft_limit_mb <= self.memory_hard_limit_mb
                && self.memory_hard_limit_mb <= 1024 * 1024,
            "invalid memory limits"
        );
        anyhow::ensure!(
            matches!(self.paste_modifier.as_str(), "" | "auto" | "cmd" | "ctrl"),
            "invalid paste modifier"
        );
        anyhow::ensure!(
            self.auto_pause_idle_seconds == 0
                || (60..=7 * 24 * 60 * 60).contains(&self.auto_pause_idle_seconds),
            "invalid idle auto-pause interval"
        );
        Ok(())
    }
}

impl ConfigHandoff {
    fn new(config: Config) -> anyhow::Result<Self> {
        let payload_hash = hash_config(&config)?;
        Ok(Self {
            schema: HANDOFF_CONFIG_SCHEMA,
            source_platform: std::env::consts::OS.into(),
            config,
            payload_hash,
        })
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == HANDOFF_CONFIG_SCHEMA,
            "unsupported handoff schema"
        );
        anyhow::ensure!(
            !self.source_platform.is_empty()
                && self.source_platform.len() <= 64
                && self
                    .source_platform
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "invalid source platform"
        );
        anyhow::ensure!(
            self.payload_hash == hash_config(&self.config)?,
            "handoff checksum mismatch"
        );
        self.config.validate()
    }
}

fn hash_config(config: &Config) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(config)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub(crate) fn requested() -> anyhow::Result<Option<ConfigCommand>> {
    parse_requested(std::env::args().skip(1))
}

fn parse_requested(
    arguments: impl IntoIterator<Item = String>,
) -> anyhow::Result<Option<ConfigCommand>> {
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("config") {
        return Ok(None);
    }
    match arguments.next().as_deref() {
        Some("export") => {
            let path = arguments.next().map(PathBuf::from);
            anyhow::ensure!(
                arguments.next().is_none(),
                "usage: vbuff config export [file]"
            );
            Ok(Some(ConfigCommand::Export(path)))
        }
        Some("apply") => {
            let path = arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("usage: vbuff config apply <file|->"))?;
            anyhow::ensure!(
                arguments.next().is_none(),
                "usage: vbuff config apply <file|->"
            );
            Ok(Some(ConfigCommand::Apply(path)))
        }
        Some("handoff") => {
            match arguments.next().as_deref() {
                Some("export") => {
                    let path = arguments.next().map(PathBuf::from).ok_or_else(|| {
                        anyhow::anyhow!("usage: vbuff config handoff export <file>")
                    })?;
                    anyhow::ensure!(
                        arguments.next().is_none(),
                        "usage: vbuff config handoff export <file>"
                    );
                    Ok(Some(ConfigCommand::HandoffExport(path)))
                }
                Some("apply") => {
                    let path = arguments.next().map(PathBuf::from).ok_or_else(|| {
                        anyhow::anyhow!("usage: vbuff config handoff apply <file>")
                    })?;
                    anyhow::ensure!(
                        arguments.next().is_none(),
                        "usage: vbuff config handoff apply <file>"
                    );
                    Ok(Some(ConfigCommand::HandoffApply(path)))
                }
                _ => anyhow::bail!(
                    "usage: vbuff config handoff export <file> | vbuff config handoff apply <file>"
                ),
            }
        }
        Some("migrate") => {
            let command = match arguments.next().as_deref() {
                Some("preview") => ConfigCommand::MigratePreview,
                Some("apply") => ConfigCommand::MigrateApply,
                _ => anyhow::bail!(
                    "usage: vbuff config migrate preview | vbuff config migrate apply"
                ),
            };
            anyhow::ensure!(
                arguments.next().is_none(),
                "usage: vbuff config migrate preview | vbuff config migrate apply"
            );
            Ok(Some(command))
        }
        _ => anyhow::bail!(
            "usage: vbuff config export [file] | vbuff config apply <file|-> | vbuff config handoff <export|apply> <file> | vbuff config migrate <preview|apply>"
        ),
    }
}

pub(crate) fn run(command: ConfigCommand) -> anyhow::Result<()> {
    match command {
        ConfigCommand::Export(path) => {
            let text = toml::to_string_pretty(&Config::load_for_inspection()?.shareable())?;
            if let Some(path) = path {
                if let Some(parent) = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, text)?;
            } else {
                print!("{text}");
            }
        }
        ConfigCommand::Apply(path) => {
            let text = if path.as_os_str() == "-" {
                read_bounded(std::io::stdin().lock())?
            } else {
                read_bounded(std::fs::File::open(path)?)?
            };
            let shared: ShareableConfig = toml::from_str(&text)?;
            let mut config = Config::load_or_create()?;
            config.apply_shareable(shared)?;
            config.save()?;
            println!("vbuff config: applied shareable settings");
        }
        ConfigCommand::HandoffExport(path) => {
            let handoff = ConfigHandoff::new(Config::load_for_inspection()?)?;
            write_private(&path, &toml::to_string_pretty(&handoff)?)?;
            println!("vbuff config: wrote full setup handoff");
        }
        ConfigCommand::HandoffApply(path) => {
            let text = read_bounded(std::fs::File::open(path)?)?;
            let handoff = parse_runtime_handoff(&text)?;
            handoff.validate()?;
            handoff.config.save()?;
            println!("vbuff config: applied full setup handoff");
        }
        ConfigCommand::MigratePreview => {
            let path = config_path()?;
            let text = read_bounded(std::fs::File::open(&path)?)?;
            let preview = preview_migration(&text)?;
            print_migration_preview(&preview, &rollback_path(&path, preview.from_schema));
        }
        ConfigCommand::MigrateApply => {
            let path = config_path()?;
            let text = read_bounded(std::fs::File::open(&path)?)?;
            let preview = preview_migration(&text)?;
            if preview.changes.is_empty() {
                println!("vbuff config: already at schema {}", preview.to_schema);
                return Ok(());
            }
            let backup = rollback_path(&path, preview.from_schema);
            if backup.exists() {
                anyhow::ensure!(
                    read_bounded(std::fs::File::open(&backup)?)? == text,
                    "rollback copy already exists with different contents"
                );
            } else {
                write_private(&backup, &text)?;
            }
            write_private(&path, &migrate_config_text(&text)?)?;
            println!(
                "vbuff config: migrated schema {} -> {}; rollback copy written",
                preview.from_schema, preview.to_schema
            );
        }
    }
    Ok(())
}

fn preview_migration(text: &str) -> anyhow::Result<ConfigMigrationPreview> {
    let raw: toml::Value = toml::from_str(text)?;
    validate_runtime_config_keys(&raw)?;
    let from_schema = raw
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        .map(u16::try_from)
        .transpose()
        .map_err(|_| anyhow::anyhow!("invalid config schema"))?
        .unwrap_or(LEGACY_CONFIG_SCHEMA_VERSION);
    anyhow::ensure!(from_schema > 0, "invalid config schema");
    anyhow::ensure!(
        from_schema <= CONFIG_SCHEMA_VERSION,
        "config schema {from_schema} is newer than this vbuff build"
    );
    let _: Config = toml::from_str(text)?;
    let changes = if from_schema < CONFIG_SCHEMA_VERSION {
        vec![
            "record schema_version = 2",
            "recognize optional first-run profile state",
            "add idle, lock, and remote-control auto-pause policy",
        ]
    } else {
        Vec::new()
    };
    Ok(ConfigMigrationPreview {
        from_schema,
        to_schema: CONFIG_SCHEMA_VERSION,
        changes,
    })
}

fn parse_runtime_config(text: &str) -> anyhow::Result<Config> {
    let value: toml::Value = toml::from_str(text)?;
    validate_runtime_config_keys(&value)?;
    Ok(toml::from_str(text)?)
}

fn parse_runtime_handoff(text: &str) -> anyhow::Result<ConfigHandoff> {
    let value: toml::Value = toml::from_str(text)?;
    let table = value
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("handoff must be a TOML table"))?;
    let config = table
        .get("config")
        .ok_or_else(|| anyhow::anyhow!("handoff is missing config"))?;
    validate_runtime_config_keys(config)?;
    Ok(toml::from_str(text)?)
}

fn validate_runtime_config_keys(value: &toml::Value) -> anyhow::Result<()> {
    const CONFIG_KEYS: &[&str] = &[
        "schema_version",
        "hotkey",
        "poll_interval_ms",
        "max_history",
        "paste_modifier",
        "excluded_apps",
        "source_rules",
        "skip_whitespace_only",
        "detect_secrets",
        "secret_ttl_seconds",
        "capture_soft_limit_bytes",
        "capture_hard_limit_bytes",
        "capture_preview_bytes",
        "memory_soft_limit_mb",
        "memory_hard_limit_mb",
        "strict_security_mode",
        "launch_at_login",
        "default_profile",
        "auto_pause_idle_seconds",
        "auto_pause_on_lock",
        "auto_pause_remote",
        "hotkey_coachmark_seen",
        "ui_density",
        "ui_reduced_motion",
        "ui_large_preview",
        "ui_handed_mode",
        "ui_motion_inspector",
        "ui_show_health_digest",
    ];
    const SOURCE_RULE_KEYS: &[&str] = &["app_contains", "title_regex", "url_host_suffix", "action"];

    let table = value
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("config must be a TOML table"))?;
    for key in table.keys() {
        anyhow::ensure!(
            CONFIG_KEYS.contains(&key.as_str()),
            "unknown config key `{key}`"
        );
    }
    if let Some(rules) = table.get("source_rules") {
        let rules = rules
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("source_rules must be an array"))?;
        for rule in rules {
            let rule = rule
                .as_table()
                .ok_or_else(|| anyhow::anyhow!("source rule must be a TOML table"))?;
            for key in rule.keys() {
                anyhow::ensure!(
                    SOURCE_RULE_KEYS.contains(&key.as_str()),
                    "unknown source rule key `{key}`"
                );
            }
        }
    }
    Ok(())
}

fn migrate_config_text(text: &str) -> anyhow::Result<String> {
    let config = parse_runtime_config(text)?;
    let mut document = text.parse::<toml_edit::Document>()?;
    document["schema_version"] = toml_edit::value(i64::from(CONFIG_SCHEMA_VERSION));
    if document.get("auto_pause_idle_seconds").is_none() {
        document["auto_pause_idle_seconds"] =
            toml_edit::value(i64::try_from(config.auto_pause_idle_seconds)?);
    }
    if document.get("auto_pause_on_lock").is_none() {
        document["auto_pause_on_lock"] = toml_edit::value(config.auto_pause_on_lock);
    }
    if document.get("auto_pause_remote").is_none() {
        document["auto_pause_remote"] = toml_edit::value(config.auto_pause_remote);
    }
    Ok(document.to_string())
}

fn rollback_path(path: &std::path::Path, schema: u16) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".schema-{schema}.rollback"));
    PathBuf::from(value)
}

fn print_migration_preview(preview: &ConfigMigrationPreview, backup: &std::path::Path) {
    if preview.changes.is_empty() {
        println!("vbuff config: already at schema {}", preview.to_schema);
        return;
    }
    println!(
        "vbuff config migration preview: schema {} -> {}",
        preview.from_schema, preview.to_schema
    );
    for change in &preview.changes {
        println!("- {change}");
    }
    println!("- rollback copy: {}", backup.display());
}

fn write_private(path: &std::path::Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = atomic_write_file::AtomicWriteFile::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.commit()?;
    Ok(())
}

fn read_bounded(reader: impl std::io::Read) -> anyhow::Result<String> {
    let mut text = String::new();
    reader
        .take((MAX_SHAREABLE_CONFIG_BYTES + 1) as u64)
        .read_to_string(&mut text)?;
    anyhow::ensure!(
        text.len() <= MAX_SHAREABLE_CONFIG_BYTES,
        "shareable config exceeds 256 KiB"
    );
    Ok(text)
}

/// The config file path: `<config_dir>/vbuff/config.toml`.
pub fn config_path() -> anyhow::Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?;
    Ok(dir.join("vbuff").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrips_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg.hotkey, back.hotkey);
        assert_eq!(cfg.max_history, back.max_history);
        assert_eq!(cfg.launch_at_login, back.launch_at_login);
        assert_eq!(back.schema_version, CONFIG_SCHEMA_VERSION);
    }

    #[test]
    fn shareable_config_omits_private_matchers_and_preserves_them_on_apply() {
        let source_rule = SourceRuleConfig {
            app_contains: Some("private-app".into()),
            ..SourceRuleConfig::default()
        };
        let mut config = Config {
            excluded_apps: vec!["secret-client".into()],
            source_rules: vec![source_rule],
            ..Config::default()
        };
        let text = toml::to_string_pretty(&config.shareable()).unwrap();
        assert!(!text.contains("secret-client"));
        assert!(!text.contains("private-app"));

        let mut shared: ShareableConfig = toml::from_str(&text).unwrap();
        shared.max_history = 321;
        config.apply_shareable(shared).unwrap();
        assert_eq!(config.max_history, 321);
        assert_eq!(config.excluded_apps, vec!["secret-client"]);
        assert_eq!(
            config.source_rules[0].app_contains.as_deref(),
            Some("private-app")
        );
    }

    #[test]
    fn full_handoff_is_checksummed_and_keeps_private_rules_explicit() {
        let config = Config {
            excluded_apps: vec!["private-app".into()],
            source_rules: vec![SourceRuleConfig {
                title_regex: Some("(?i)bank".into()),
                ..SourceRuleConfig::default()
            }],
            ..Config::default()
        };
        let handoff = ConfigHandoff::new(config).unwrap();
        handoff.validate().unwrap();
        let text = toml::to_string(&handoff).unwrap();
        assert!(text.contains("private-app"));
        let mut tampered: ConfigHandoff = toml::from_str(&text).unwrap();
        tampered.config.max_history += 1;
        assert!(tampered.validate().is_err());
        assert!(!format!("{handoff:?}").contains("private-app"));
    }

    #[test]
    fn handoff_cli_requires_an_explicit_file_and_verb() {
        assert_eq!(
            parse_requested([
                "config".into(),
                "handoff".into(),
                "export".into(),
                "setup.toml".into(),
            ])
            .unwrap(),
            Some(ConfigCommand::HandoffExport(PathBuf::from("setup.toml")))
        );
        assert!(parse_requested(["config".into(), "handoff".into()]).is_err());
    }

    #[test]
    fn shareable_config_rejects_unsafe_limits() {
        let mut shared = Config::default().shareable();
        shared.capture_soft_limit_bytes = shared.capture_hard_limit_bytes + 1;
        assert!(Config::default().apply_shareable(shared).is_err());
    }

    #[test]
    fn runtime_config_rejects_invalid_private_regex_without_echoing_it() {
        let mut config = Config::default();
        config.source_rules.push(SourceRuleConfig {
            title_regex: Some("(?P<private-pattern>".into()),
            ..SourceRuleConfig::default()
        });

        let error = config.validate().unwrap_err().to_string();
        assert_eq!(error, "invalid source rule regex");
        assert!(!error.contains("private-pattern"));
    }

    #[test]
    fn runtime_config_rejects_top_level_and_nested_security_typos() {
        let top_level = "schema_version = 2\nstrict_security_mod = true\n";
        assert!(
            parse_runtime_config(top_level)
                .unwrap_err()
                .to_string()
                .contains("strict_security_mod")
        );

        let nested = r#"
schema_version = 2
[[source_rules]]
app_contians = "private-app"
action = "skip"
"#;
        assert!(
            parse_runtime_config(nested)
                .unwrap_err()
                .to_string()
                .contains("app_contians")
        );
    }

    #[test]
    fn config_migration_rejects_unknown_keys_before_writing() {
        let typo = "schema_version = 1\nstrict_security_mod = true\n";
        assert!(preview_migration(typo).is_err());
        assert!(migrate_config_text(typo).is_err());
    }

    #[test]
    fn native_ui_preferences_roundtrip_through_owner_config() {
        let mut config = Config::default();
        let preferences = UiPreferences {
            density: DensityMode::Compact,
            reduced_motion: true,
            large_preview: false,
            handed_mode: HandedMode::Left,
            motion_inspector: true,
            show_health_digest: true,
        };

        config.apply_ui_preferences(&preferences, true);
        config.validate().unwrap();
        assert_eq!(config.ui_preferences(), preferences);
    }

    #[test]
    fn unrelated_ui_change_preserves_os_reduced_motion_inheritance() {
        let mut config = Config::default();
        assert_eq!(config.ui_reduced_motion, None);
        let mut preferences = config.ui_preferences();
        preferences.density = DensityMode::Compact;

        config.apply_ui_preferences(&preferences, false);

        assert_eq!(config.ui_reduced_motion, None);
        preferences.reduced_motion = !preferences.reduced_motion;
        config.apply_ui_preferences(&preferences, true);
        assert_eq!(config.ui_reduced_motion, Some(preferences.reduced_motion));
    }

    #[test]
    fn command_parser_rejects_ambiguous_trailing_arguments() {
        assert!(
            parse_requested(["config", "export", "one.toml", "two.toml"].map(str::to_owned))
                .is_err()
        );
        assert!(
            parse_requested(["config", "apply", "one.toml", "extra"].map(str::to_owned)).is_err()
        );
    }

    #[test]
    fn shareable_config_reader_is_bounded() {
        let oversized = vec![b'x'; MAX_SHAREABLE_CONFIG_BYTES + 1];
        assert!(read_bounded(&oversized[..]).is_err());
    }

    #[test]
    fn profiles_apply_coherent_bounded_defaults() {
        let mut config = Config::default();
        config.apply_default_profile(DefaultProfile::PrivacyMax);
        assert_eq!(config.default_profile, Some(DefaultProfile::PrivacyMax));
        assert_eq!(config.max_history, 200);
        assert_eq!(config.secret_ttl_seconds, 60);
        assert_eq!(config.auto_pause_idle_seconds, 5 * 60);
        assert!(config.auto_pause_remote);
        config.shareable().validate().unwrap();
    }

    #[test]
    fn legacy_config_migration_is_previewed_without_private_values() {
        let legacy = r#"
hotkey = "Ctrl+Shift+V"
excluded_apps = ["private-bank-app"]
auto_pause_idle_seconds = 900
"#;
        let parsed: Config = toml::from_str(legacy).unwrap();
        assert_eq!(parsed.schema_version, LEGACY_CONFIG_SCHEMA_VERSION);
        assert!(parsed.save().is_err());
        let preview = preview_migration(legacy).unwrap();
        assert_eq!(preview.from_schema, 1);
        assert_eq!(preview.to_schema, 2);
        assert_eq!(preview.changes.len(), 3);
        assert!(!format!("{preview:?}").contains("private-bank-app"));
        assert_eq!(
            rollback_path(std::path::Path::new("config.toml"), 1),
            PathBuf::from("config.toml.schema-1.rollback")
        );
        assert!(preview_migration("schema_version = -1").is_err());
        assert!(preview_migration("schema_version = 65536").is_err());

        let migrated = migrate_config_text(
            "# keep this comment\nhotkey = \"Ctrl+Shift+V\"\nexcluded_apps = [\"private-bank-app\"]\n",
        )
        .unwrap();
        assert!(migrated.contains("# keep this comment"));
        assert!(migrated.contains("excluded_apps = [\"private-bank-app\"]"));
        assert!(migrated.contains("schema_version = 2"));
        assert!(migrated.contains("auto_pause_on_lock = true"));
    }

    #[test]
    fn migrate_cli_requires_an_explicit_non_destructive_verb() {
        assert_eq!(
            parse_requested(["config", "migrate", "preview"].map(str::to_owned)).unwrap(),
            Some(ConfigCommand::MigratePreview)
        );
        assert_eq!(
            parse_requested(["config", "migrate", "apply"].map(str::to_owned)).unwrap(),
            Some(ConfigCommand::MigrateApply)
        );
        assert!(parse_requested(["config", "migrate"].map(str::to_owned)).is_err());
    }
}
