//! Application configuration, persisted as TOML.
//!
//! The config lives at `<config_dir>/vbuff/config.toml`. It is loaded at start
//! and created with defaults if missing. Policy (hotkey, intervals, exclusions)
//! lives here, not in the database.

use std::io::Read as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const SHAREABLE_CONFIG_SCHEMA: u16 = 1;
const MAX_SHAREABLE_CONFIG_BYTES: usize = 256 * 1024;

/// User-tunable configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
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
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceRuleConfig {
    pub app_contains: Option<String>,
    pub title_regex: Option<String>,
    pub url_host_suffix: Option<String>,
    pub action: SourceRuleAction,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConfigCommand {
    Export(Option<PathBuf>),
    Apply(PathBuf),
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
            let text = std::fs::read_to_string(&path)?;
            let cfg: Config = toml::from_str(&text)?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.save()?;
            Ok(cfg)
        }
    }

    /// Load existing policy for read-only diagnostics without creating a file.
    pub fn load_for_inspection() -> anyhow::Result<Config> {
        let path = config_path()?;
        if path.exists() {
            Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
        } else {
            Ok(Config::default())
        }
    }

    /// Persist the config to the default path.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
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
        Ok(())
    }
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
        _ => anyhow::bail!("usage: vbuff config export [file] | vbuff config apply <file|->"),
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
    }
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
    fn shareable_config_rejects_unsafe_limits() {
        let mut shared = Config::default().shareable();
        shared.capture_soft_limit_bytes = shared.capture_hard_limit_bytes + 1;
        assert!(Config::default().apply_shareable(shared).is_err());
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
}
