#![cfg(target_os = "linux")]

use std::sync::{Arc, Mutex};

use egui_kittest::{Harness, SnapshotOptions};
use vbuff_core::content_hash_from_flavors;
use vbuff_gui::{AppState, PopupApp};
use vbuff_types::{
    CapabilityView, CapabilityViewLevel, Clip, ClipId, ClipMeta, ContentKind, Flavor,
    PrivacyDecisionLevel, PrivacyEventSummary, PrivacyLedgerSummary, SecurityPostureLevel,
    SecurityPostureSummary, SloMetricState,
};

#[test]
fn popup_golden_matrix_covers_themes_dpi_and_trust_states() {
    let snapshots = SnapshotOptions::new()
        .output_path(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots"));
    for (theme_name, theme) in [("light", egui::Theme::Light), ("dark", egui::Theme::Dark)] {
        for (dpi_name, pixels_per_point) in [("1x", 1.0_f32), ("2x", 2.0_f32)] {
            for surface in [Surface::Empty, Surface::Populated, Surface::Trust] {
                let name = format!("popup_{theme_name}_{dpi_name}_{}", surface.name());
                let state = Arc::new(Mutex::new(snapshot_state(surface)));
                let mut harness = Harness::builder()
                    .with_size(egui::vec2(560.0, 620.0))
                    .with_pixels_per_point(pixels_per_point)
                    .with_theme(theme)
                    .wgpu()
                    .build_eframe(|_| PopupApp::new(state));
                if surface == Surface::Trust {
                    let ctx = harness.ctx.clone();
                    harness.state_mut().request_trust_view(&ctx);
                    harness.run_steps(2);
                }
                harness.snapshot_options(name, &snapshots);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Empty,
    Populated,
    Trust,
}

impl Surface {
    fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Populated => "populated",
            Self::Trust => "trust",
        }
    }
}

fn snapshot_state(surface: Surface) -> AppState {
    let clips = if surface == Surface::Empty {
        Vec::new()
    } else {
        vec![
            text_clip(
                "cargo test --workspace --all-features --locked",
                ContentKind::Code,
            ),
            text_clip("https://github.com/vbuff/vbuff", ContentKind::Url),
            text_clip("Release notes and next actions", ContentKind::Text),
        ]
    };
    let mut state = AppState::with_clips(clips);
    state.show_requested = true;
    state.capture_health = vbuff_types::CaptureHealth::Watching;
    state.capture_stats.captured = 42;
    state.capture_stats.intentionally_skipped = 3;
    state.slo_status.zero_loss = SloMetricState::Met;
    state.security_posture = SecurityPostureSummary {
        level: SecurityPostureLevel::Partial,
        active: 3,
        degraded: 2,
        unavailable: 1,
        strict_mode: false,
    };
    state.capabilities = vec![
        CapabilityView {
            feature: "core_dumps".into(),
            level: CapabilityViewLevel::Active,
            detail: "process core-dump limit is zero".into(),
        },
        CapabilityView {
            feature: "foreground_identity".into(),
            level: CapabilityViewLevel::Degraded,
            detail: "generic backend has no authoritative foreground-app probe".into(),
        },
        CapabilityView {
            feature: "encryption_at_rest".into(),
            level: CapabilityViewLevel::Unavailable,
            detail: "bundled SQLite is not SQLCipher".into(),
        },
    ];
    state.privacy_ledger = PrivacyLedgerSummary {
        chain_valid: true,
        head_hash_prefix: "a1b2c3d4e5f6".into(),
        recent: vec![
            PrivacyEventSummary {
                sequence: 7,
                timestamp_ms: 1_700_000_000_000,
                count: 1,
                decision: PrivacyDecisionLevel::Captured,
                reason: "captured".into(),
            },
            PrivacyEventSummary {
                sequence: 6,
                timestamp_ms: 1_699_999_999_000,
                count: 2,
                decision: PrivacyDecisionLevel::Skipped,
                reason: "concealed".into(),
            },
        ],
    };
    state
}

fn text_clip(text: &str, kind: ContentKind) -> Clip {
    let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
    Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(kind, text.len() as u64, Some("snapshot.fixture".into())),
        pinned: false,
        favorite: false,
    }
}
