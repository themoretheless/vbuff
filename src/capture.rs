//! Clipboard capture supervision and capture-policy evaluation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_platform::{ArboardClipboard, CapturedClipboard, ClipboardBackend};
use vbuff_types::{Clip, ClipId, ClipMeta};

use crate::config::Config;
use crate::history::History;

/// The pure, inexpensive rules that run before a clip reaches storage.
#[derive(Clone, Debug)]
struct CapturePolicy {
    skip_whitespace_only: bool,
    excluded_apps: Vec<String>,
}

impl From<&Config> for CapturePolicy {
    fn from(config: &Config) -> Self {
        Self {
            skip_whitespace_only: config.skip_whitespace_only,
            excluded_apps: config
                .excluded_apps
                .iter()
                .filter(|app| !app.is_empty())
                .map(|app| app.to_lowercase())
                .collect(),
        }
    }
}

impl CapturePolicy {
    fn accepts(&self, captured: &CapturedClipboard) -> bool {
        if self.skip_whitespace_only
            && captured
                .flavors
                .iter()
                .find_map(|flavor| flavor.as_text())
                .is_some_and(|text| text.trim().is_empty())
        {
            return false;
        }

        if captured.source_app.as_ref().is_some_and(|source| {
            let source = source.to_lowercase();
            self.excluded_apps
                .iter()
                .any(|excluded| source.contains(excluded))
        }) {
            return false;
        }

        true
    }
}

/// Start the one background capture worker used by the single-process MVP.
pub(crate) fn spawn(
    history: History,
    paused: Arc<AtomicBool>,
    config: Config,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut clipboard = match ArboardClipboard::new() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                tracing::error!("clipboard backend unavailable: {error}");
                return;
            }
        };

        let policy = CapturePolicy::from(&config);
        let interval = Duration::from_millis(config.poll_interval_ms.max(50));
        let mut last_hash: Option<[u8; 32]> = None;

        loop {
            std::thread::sleep(interval);

            if paused.load(Ordering::Relaxed) {
                continue;
            }

            let captured = match clipboard.read() {
                Ok(captured) if !captured.is_empty() => captured,
                Ok(_) | Err(_) => continue,
            };
            let hash = content_hash_from_flavors(&captured.flavors);
            if last_hash == Some(hash) {
                continue;
            }

            if !policy.accepts(&captured) {
                last_hash = Some(hash);
                continue;
            }

            let clip = build_clip(captured, hash);
            match history.insert(&clip, config.max_history) {
                Ok(()) => last_hash = Some(hash),
                Err(error) => {
                    // Keep the previous hash so the same clipboard is retried on
                    // the next poll instead of being silently lost.
                    tracing::warn!("capture insert failed: {error}");
                }
            }
        }
    })
}

fn build_clip(captured: CapturedClipboard, content_hash: [u8; 32]) -> Clip {
    let kind = detect_kind(&captured.flavors);
    let byte_size = captured
        .flavors
        .iter()
        .map(|flavor| flavor.body.byte_size())
        .sum();

    Clip {
        id: ClipId::new(),
        flavors: captured.flavors,
        content_hash,
        meta: ClipMeta::now(kind, byte_size, captured.source_app),
        pinned: false,
        favorite: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::Flavor;

    fn captured(text: &str, source_app: Option<&str>) -> CapturedClipboard {
        CapturedClipboard {
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            source_app: source_app.map(str::to_owned),
        }
    }

    #[test]
    fn policy_rejects_whitespace_and_excluded_apps() {
        let config = Config {
            excluded_apps: vec!["onepassword".into()],
            ..Default::default()
        };
        let policy = CapturePolicy::from(&config);

        assert!(!policy.accepts(&captured("  \n", None)));
        assert!(!policy.accepts(&captured("secret", Some("com.AgileBits.OnePassword7"))));
        assert!(policy.accepts(&captured("hello", Some("com.apple.Safari"))));
    }

    #[test]
    fn build_clip_preserves_source_and_byte_count() {
        let captured = captured("hello", Some("editor.app"));
        let hash = content_hash_from_flavors(&captured.flavors);
        let clip = build_clip(captured, hash);

        assert_eq!(clip.meta.byte_size, 5);
        assert_eq!(clip.meta.source_app.as_deref(), Some("editor.app"));
        assert_eq!(clip.content_hash, hash);
    }
}
