//! Fake-data state used by the browser demo; no clipboard or store access.

use vbuff_core::content_hash_from_flavors;
use vbuff_types::{
    CapabilityView, CapabilityViewLevel, CaptureHealth, Clip, ClipId, ClipMeta, ContentKind,
    Flavor, SecurityPostureLevel, SecurityPostureSummary,
};

use crate::AppState;

pub fn demo_state() -> AppState {
    let mut state = AppState::with_clips(vec![
        text_clip("cargo test --workspace --all-features", ContentKind::Code),
        text_clip("https://github.com/themoretheless/vbuff", ContentKind::Url),
        text_clip("Release notes and next actions", ContentKind::Text),
        text_clip("#2f7d67", ContentKind::Color),
    ]);
    state.show_requested = true;
    state.capture_health = CaptureHealth::Watching;
    state.capture_stats.captured = 24;
    state.capture_stats.intentionally_skipped = 2;
    state.security_posture = SecurityPostureSummary {
        level: SecurityPostureLevel::Partial,
        active: 3,
        degraded: 1,
        unavailable: 0,
        strict_mode: false,
    };
    state.capabilities = vec![CapabilityView {
        feature: "browser_demo".into(),
        level: CapabilityViewLevel::Degraded,
        detail: "fake clips only; clipboard, persistence, paste, and network are disabled".into(),
    }];
    state
}

fn text_clip(text: &str, kind: ContentKind) -> Clip {
    let flavors = vec![Flavor::inline(
        "text/plain;charset=utf-8",
        text.as_bytes().to_vec(),
    )];
    Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(kind, text.len() as u64, Some("demo.fixture".into())),
        pinned: false,
        favorite: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_explicitly_fake_and_has_no_security_greenwash() {
        let state = demo_state();
        assert_eq!(state.clips.len(), 4);
        assert_eq!(state.security_posture.level, SecurityPostureLevel::Partial);
        assert!(state.capabilities[0].detail.contains("fake clips only"));
        assert!(state.clips.iter().all(|clip| !clip.meta.ai_allowed));
    }
}
