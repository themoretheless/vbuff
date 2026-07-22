//! Read-only Trust surface for privacy posture and release evidence.

use chrono::Utc;
use egui::RichText;
use vbuff_core::trust::{PrivacyScore, PrivacyScoreLevel};
use vbuff_types::{
    CapabilityView, CapabilityViewLevel, PrivacyDecisionLevel, PrivacyLedgerSummary,
    SecurityPostureLevel, SecurityPostureSummary, SloMetricState, SloStatusSummary,
};

use crate::design;

pub(crate) fn render(
    ui: &mut egui::Ui,
    posture: SecurityPostureSummary,
    capabilities: &[CapabilityView],
    ledger: &PrivacyLedgerSummary,
    privacy_score: Option<&PrivacyScore>,
    slo: &SloStatusSummary,
) {
    let posture_color = security_color(ui, posture.level);
    egui::Frame::new()
        .fill(posture_color.gamma_multiply(0.10))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                design::status_dot(ui, posture_color);
                let posture_label = match posture.level {
                    SecurityPostureLevel::Protected => "Protection available",
                    SecurityPostureLevel::Partial => "Protection incomplete",
                    SecurityPostureLevel::Blocked => "Protection blocked",
                };
                ui.label(RichText::new(posture_label).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small(format!(
                        "{} active · {} degraded · {} unavailable",
                        posture.active, posture.degraded, posture.unavailable
                    ));
                });
            });
        });
    ui.add_space(6.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_privacy_score(ui, privacy_score);
            ui.add_space(16.0);
            render_attention_items(ui, capabilities);
            ui.add_space(16.0);
            render_privacy_ledger(ui, ledger);
            ui.add_space(16.0);
            render_technical_diagnostics(ui, capabilities, ledger, slo);
        });
}

fn render_privacy_score(ui: &mut egui::Ui, score: Option<&PrivacyScore>) {
    design::section_heading(ui, "Privacy posture", Some("configuration estimate"));
    let Some(score) = score else {
        ui.label(RichText::new("Posture inputs unavailable").weak());
        return;
    };

    let color = privacy_score_color(ui, score.level);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} / 100", score.value))
                .size(20.0)
                .strong()
                .color(color),
        );
        ui.label(
            RichText::new(privacy_score_label(score.level))
                .small()
                .color(color),
        );
    });
    egui::Grid::new("privacy_score_factors")
        .num_columns(2)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for factor in &score.factors {
                ui.label(RichText::new(factor.key.replace('_', " ")).small().weak());
                let color = if factor.points < 0 {
                    design::danger(ui)
                } else {
                    design::success(ui)
                };
                ui.label(
                    RichText::new(format!("{:+}", factor.points))
                        .small()
                        .monospace()
                        .color(color),
                );
                ui.end_row();
            }
        });
    ui.label(
        RichText::new("This estimate is not a security guarantee.")
            .small()
            .weak(),
    );
}

fn render_attention_items(ui: &mut egui::Ui, capabilities: &[CapabilityView]) {
    design::section_heading(ui, "Needs attention", None);
    let mut issues = capabilities.iter().filter(|capability| {
        matches!(
            capability.level,
            CapabilityViewLevel::Degraded | CapabilityViewLevel::Unavailable
        )
    });
    let Some(first) = issues.next() else {
        ui.label(RichText::new("No detected platform gaps").weak());
        return;
    };

    for capability in std::iter::once(first).chain(issues) {
        let color = capability_color(ui, capability.level);
        ui.horizontal(|ui| {
            design::status_dot(ui, color);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(capability.feature.replace('_', " ")).strong());
                    ui.label(RichText::new(capability.level.label()).small().color(color));
                });
                ui.label(RichText::new(&capability.detail).small().weak());
            });
        });
    }
}

fn render_privacy_ledger(ui: &mut egui::Ui, ledger: &PrivacyLedgerSummary) {
    ui.horizontal(|ui| {
        design::section_heading(ui, "This session's capture log", None);
        if !ledger.recent.is_empty() {
            let color = if ledger.chain_valid {
                design::success(ui)
            } else {
                design::danger(ui)
            };
            ui.label(
                RichText::new(if ledger.chain_valid {
                    "Log chain intact"
                } else {
                    "Log chain invalid"
                })
                .small()
                .color(color),
            );
        }
    });
    if ledger.recent.is_empty() {
        ui.label(RichText::new("No session decisions recorded yet").weak());
        return;
    }

    for event in &ledger.recent {
        let color = privacy_color(ui, event.decision);
        let time = chrono::DateTime::<Utc>::from_timestamp_millis(event.timestamp_ms as i64)
            .map(|value| value.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "--:--:--".into());
        ui.horizontal(|ui| {
            design::status_dot(ui, color);
            ui.monospace(format!("#{:04}", event.sequence));
            let decision_label = match event.decision {
                PrivacyDecisionLevel::Captured => "Stored",
                PrivacyDecisionLevel::Skipped => "Not stored by policy",
                PrivacyDecisionLevel::Lost => "Lost unexpectedly",
            };
            ui.label(RichText::new(decision_label).color(color));
            ui.label(format!("{} ×{}", event.reason, event.count));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(time).small().weak());
            });
        });
    }
    ui.label(
        RichText::new("Session-only diagnostic log; it does not prove OS or storage protection.")
            .small()
            .weak(),
    );
}

fn render_technical_diagnostics(
    ui: &mut egui::Ui,
    capabilities: &[CapabilityView],
    ledger: &PrivacyLedgerSummary,
    slo: &SloStatusSummary,
) {
    egui::CollapsingHeader::new("Technical diagnostics")
        .default_open(false)
        .show(ui, |ui| {
            design::section_heading(ui, "This session's runtime signals", None);
            egui::Grid::new("trust_slo_grid")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    render_slo_row(ui, "Zero lost captures", slo.zero_loss, "budget 0");
                    render_slo_row(ui, "Search p99", slo.search_latency, "budget 16 ms");
                    render_slo_row(ui, "Idle CPU", slo.idle_cpu, "budget 0.5%");
                    render_slo_row(ui, "Login ready", slo.login_ready, "budget 500 ms");
                });

            ui.add_space(12.0);
            design::section_heading(ui, "Current platform evidence", None);
            if capabilities.is_empty() {
                ui.label(RichText::new("No capability evidence published").weak());
            } else {
                for capability in capabilities {
                    let color = capability_color(ui, capability.level);
                    ui.horizontal_wrapped(|ui| {
                        design::status_dot(ui, color);
                        ui.label(capability.feature.replace('_', " "));
                        ui.label(RichText::new(capability.level.label()).small().color(color));
                        ui.label(RichText::new(&capability.detail).small().weak());
                    });
                }
            }

            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label(
                    RichText::new(format!("ledger {}", ledger.head_hash_prefix))
                        .small()
                        .monospace(),
                );
                ui.separator();
                ui.label(RichText::new("Current build verification: not checked").small());
            });
        });
}

fn security_color(ui: &egui::Ui, level: SecurityPostureLevel) -> egui::Color32 {
    match level {
        SecurityPostureLevel::Protected => design::success(ui),
        SecurityPostureLevel::Partial => design::warning(ui),
        SecurityPostureLevel::Blocked => design::danger(ui),
    }
}

fn capability_color(ui: &egui::Ui, level: CapabilityViewLevel) -> egui::Color32 {
    match level {
        CapabilityViewLevel::Active => design::success(ui),
        CapabilityViewLevel::Degraded => design::warning(ui),
        CapabilityViewLevel::Unavailable => design::danger(ui),
        CapabilityViewLevel::NotApplicable => ui.visuals().weak_text_color(),
    }
}

fn privacy_color(ui: &egui::Ui, level: PrivacyDecisionLevel) -> egui::Color32 {
    match level {
        PrivacyDecisionLevel::Captured => design::info(ui),
        PrivacyDecisionLevel::Skipped => design::success(ui),
        PrivacyDecisionLevel::Lost => design::danger(ui),
    }
}

const fn privacy_score_label(level: PrivacyScoreLevel) -> &'static str {
    match level {
        PrivacyScoreLevel::Strong => "Strong",
        PrivacyScoreLevel::Balanced => "Balanced",
        PrivacyScoreLevel::NeedsAttention => "Needs attention",
    }
}

fn privacy_score_color(ui: &egui::Ui, level: PrivacyScoreLevel) -> egui::Color32 {
    match level {
        PrivacyScoreLevel::Strong => design::success(ui),
        PrivacyScoreLevel::Balanced => design::warning(ui),
        PrivacyScoreLevel::NeedsAttention => design::danger(ui),
    }
}

fn render_slo_row(ui: &mut egui::Ui, label: &str, state: SloMetricState, budget: &str) {
    let color = match state {
        SloMetricState::Met => design::success(ui),
        SloMetricState::Breached => design::danger(ui),
        SloMetricState::Unknown => ui.visuals().weak_text_color(),
    };
    ui.label(label);
    ui.label(RichText::new(state.label()).small().color(color));
    ui.label(RichText::new(budget).small().weak());
    ui.end_row();
}
