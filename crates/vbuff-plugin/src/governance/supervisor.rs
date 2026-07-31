use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use super::{invalid, valid_id};
use crate::{PluginError, Result};

const MAX_FAILURES: usize = 256;
const MAX_SUPERVISED_PLUGINS: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeState {
    Ready,
    DisabledAfterPanic,
    DisabledByPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginFailureReport {
    pub plugin_id: String,
    pub action_id: String,
    pub occurred_at_ms: i64,
    pub failure_sequence: u64,
    pub state: PluginRuntimeState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginSupervisor {
    states: BTreeMap<String, PluginRuntimeState>,
    failures: VecDeque<PluginFailureReport>,
    failure_sequence: u64,
}

impl PluginSupervisor {
    pub fn register(&mut self, plugin_id: String) -> Result<()> {
        if !valid_id(&plugin_id) {
            return invalid("supervised plugin id is invalid");
        }
        if !self.states.contains_key(&plugin_id) && self.states.len() >= MAX_SUPERVISED_PLUGINS {
            return invalid("too many supervised plugins");
        }
        self.states
            .entry(plugin_id)
            .or_insert(PluginRuntimeState::Ready);
        Ok(())
    }

    pub fn state(&self, plugin_id: &str) -> Option<PluginRuntimeState> {
        self.states.get(plugin_id).copied()
    }

    pub fn can_run(&self, plugin_id: &str) -> bool {
        self.state(plugin_id) == Some(PluginRuntimeState::Ready)
    }

    pub fn disable_by_policy(&mut self, plugin_id: &str) -> Result<()> {
        let state = self
            .states
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::InvalidBundle("plugin is not supervised".into()))?;
        *state = PluginRuntimeState::DisabledByPolicy;
        Ok(())
    }

    pub fn record_panic(
        &mut self,
        plugin_id: &str,
        action_id: &str,
        occurred_at_ms: i64,
    ) -> Result<PluginFailureReport> {
        if !valid_id(action_id) {
            return invalid("failed action id is invalid");
        }
        if occurred_at_ms < 0 {
            return invalid("plugin failure timestamp is invalid");
        }
        let state = self
            .states
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::InvalidBundle("plugin is not supervised".into()))?;
        *state = PluginRuntimeState::DisabledAfterPanic;
        self.failure_sequence = self
            .failure_sequence
            .checked_add(1)
            .ok_or_else(|| PluginError::InvalidBundle("failure sequence overflow".into()))?;
        let report = PluginFailureReport {
            plugin_id: plugin_id.to_owned(),
            action_id: action_id.to_owned(),
            occurred_at_ms,
            failure_sequence: self.failure_sequence,
            state: *state,
        };
        if self.failures.len() == MAX_FAILURES {
            self.failures.pop_front();
        }
        self.failures.push_back(report.clone());
        Ok(report)
    }

    pub fn recent_failures(&self) -> impl Iterator<Item = &PluginFailureReport> {
        self.failures.iter()
    }
}
