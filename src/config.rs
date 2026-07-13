//! Application configuration, persisted as TOML.
//!
//! The config lives at `<config_dir>/vbuff/config.toml`. It is loaded at start
//! and created with defaults if missing. Policy (hotkey, intervals, exclusions)
//! lives here, not in the database.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}
