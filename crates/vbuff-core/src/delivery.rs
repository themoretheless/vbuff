//! Deterministic delivery gates for scope, spikes, Wayland, and dogfood.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSpikeDecision {
    KeepEgui,
    SwitchToFallback,
    ContinueProbe,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiSpikeEvidence {
    pub participants: u16,
    pub preferred_alternative: u16,
    pub rtl_shaping_failures: u16,
}

impl UiSpikeEvidence {
    pub fn decide(self) -> UiSpikeDecision {
        if self.rtl_shaping_failures > 0
            || (self.participants >= 10
                && u32::from(self.preferred_alternative) * 100 > u32::from(self.participants) * 40)
        {
            UiSpikeDecision::SwitchToFallback
        } else if self.participants < 10 {
            UiSpikeDecision::ContinueProbe
        } else {
            UiSpikeDecision::KeepEgui
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaySpikeDecision {
    ContinueInternetSync,
    CutToLanOnly,
    ContinueProbe,
}

pub fn relay_spike_decision(elapsed_days: u16, ciphertext_only_proven: bool) -> RelaySpikeDecision {
    if ciphertext_only_proven {
        RelaySpikeDecision::ContinueInternetSync
    } else if elapsed_days >= 14 {
        RelaySpikeDecision::CutToLanOnly
    } else {
        RelaySpikeDecision::ContinueProbe
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncSpike {
    DiscoveryAndPairing,
    AuthenticatedTransport,
    TextReplication,
    TypedCasReplication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpikeState {
    NotStarted,
    Active,
    Passed,
    Killed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSpikeLedger {
    states: BTreeMap<SyncSpike, SpikeState>,
}

impl Default for SyncSpikeLedger {
    fn default() -> Self {
        Self {
            states: [
                SyncSpike::DiscoveryAndPairing,
                SyncSpike::AuthenticatedTransport,
                SyncSpike::TextReplication,
                SyncSpike::TypedCasReplication,
            ]
            .into_iter()
            .map(|spike| (spike, SpikeState::NotStarted))
            .collect(),
        }
    }
}

impl SyncSpikeLedger {
    pub fn state(&self, spike: SyncSpike) -> SpikeState {
        self.states[&spike]
    }

    pub fn transition(&mut self, spike: SyncSpike, state: SpikeState) -> bool {
        let current = self.state(spike);
        let allowed = matches!(
            (current, state),
            (SpikeState::NotStarted, SpikeState::Active)
                | (SpikeState::Active, SpikeState::Passed | SpikeState::Killed)
        );
        if allowed {
            self.states.insert(spike, state);
        }
        allowed
    }

    pub fn may_start(&self, spike: SyncSpike) -> bool {
        let prior = match spike {
            SyncSpike::DiscoveryAndPairing => return true,
            SyncSpike::AuthenticatedTransport => SyncSpike::DiscoveryAndPairing,
            SyncSpike::TextReplication => SyncSpike::AuthenticatedTransport,
            SyncSpike::TypedCasReplication => SyncSpike::TextReplication,
        };
        self.state(prior) == SpikeState::Passed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScopeSnapshot {
    pub workspace_crates: u16,
    pub added_mvp_milestones: u16,
    pub longest_open_milestone_days: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopePolicy {
    pub maximum_workspace_crates: u16,
    pub maximum_added_mvp_milestones: u16,
    pub maximum_open_milestone_days: u16,
}

impl Default for ScopePolicy {
    fn default() -> Self {
        Self {
            maximum_workspace_crates: 9,
            maximum_added_mvp_milestones: 1,
            maximum_open_milestone_days: 42,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScopeTripwires {
    pub crate_growth: bool,
    pub milestone_growth: bool,
    pub milestone_stalled: bool,
}

impl ScopePolicy {
    pub fn evaluate(self, snapshot: ScopeSnapshot) -> ScopeTripwires {
        ScopeTripwires {
            crate_growth: snapshot.workspace_crates > self.maximum_workspace_crates,
            milestone_growth: snapshot.added_mvp_milestones > self.maximum_added_mvp_milestones,
            milestone_stalled: snapshot.longest_open_milestone_days
                > self.maximum_open_milestone_days,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DogfoodEvidence {
    pub days_as_only_manager: u16,
    pub silent_loss_incidents: u16,
    pub wrong_target_pastes: u16,
}

impl DogfoodEvidence {
    pub fn passes(self) -> bool {
        self.days_as_only_manager >= 14
            && self.silent_loss_incidents == 0
            && self.wrong_target_pastes == 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaylandProbe {
    pub clipboard_monitor: bool,
    pub global_shortcut: bool,
    pub safe_paste_injection: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaylandSupportDecision {
    Full,
    CaptureOnSummon,
    Unsupported,
    ProbeIncomplete,
}

impl WaylandProbe {
    pub fn decide(self, probe_completed: bool) -> WaylandSupportDecision {
        if !probe_completed {
            WaylandSupportDecision::ProbeIncomplete
        } else if self.clipboard_monitor && self.global_shortcut && self.safe_paste_injection {
            WaylandSupportDecision::Full
        } else if self.global_shortcut {
            WaylandSupportDecision::CaptureOnSummon
        } else {
            WaylandSupportDecision::Unsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_kill_criteria_are_numeric_and_not_negotiable_after_expiry() {
        assert_eq!(
            UiSpikeEvidence {
                participants: 10,
                preferred_alternative: 5,
                rtl_shaping_failures: 0
            }
            .decide(),
            UiSpikeDecision::SwitchToFallback
        );
        assert_eq!(
            relay_spike_decision(14, false),
            RelaySpikeDecision::CutToLanOnly
        );
        assert_eq!(
            relay_spike_decision(2, true),
            RelaySpikeDecision::ContinueInternetSync
        );
    }

    #[test]
    fn sync_spikes_are_independently_killable_and_ordered() {
        let mut ledger = SyncSpikeLedger::default();
        assert!(!ledger.may_start(SyncSpike::AuthenticatedTransport));
        assert!(ledger.transition(SyncSpike::DiscoveryAndPairing, SpikeState::Active));
        assert!(ledger.transition(SyncSpike::DiscoveryAndPairing, SpikeState::Passed));
        assert!(ledger.may_start(SyncSpike::AuthenticatedTransport));
        assert!(ledger.transition(SyncSpike::AuthenticatedTransport, SpikeState::Active));
        assert!(ledger.transition(SyncSpike::AuthenticatedTransport, SpikeState::Killed));
        assert!(!ledger.may_start(SyncSpike::TextReplication));
    }

    #[test]
    fn scope_dogfood_and_wayland_gates_fail_closed() {
        let tripwires = ScopePolicy::default().evaluate(ScopeSnapshot {
            workspace_crates: 10,
            added_mvp_milestones: 0,
            longest_open_milestone_days: 43,
        });
        assert!(tripwires.crate_growth);
        assert!(tripwires.milestone_stalled);
        assert!(
            !DogfoodEvidence {
                days_as_only_manager: 14,
                silent_loss_incidents: 1,
                wrong_target_pastes: 0
            }
            .passes()
        );
        assert_eq!(
            WaylandProbe::default().decide(false),
            WaylandSupportDecision::ProbeIncomplete
        );
    }
}
