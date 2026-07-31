use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{invalid, valid_id};
use crate::{PluginCapability, Result};

const MAX_TEST_DURATION_MS: u64 = 60_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTestCase {
    pub schema: u16,
    pub case_id: String,
    pub manifest_hash: [u8; 32],
    pub action_id: String,
    pub fixture_hash: [u8; 32],
    pub expected_output_hash: [u8; 32],
    pub allowed_capabilities: BTreeSet<PluginCapability>,
    pub timeout_ms: u64,
}

impl PluginTestCase {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1
            || !valid_id(&self.case_id)
            || !valid_id(&self.action_id)
            || self.manifest_hash == [0; 32]
            || self.fixture_hash == [0; 32]
            || self.expected_output_hash == [0; 32]
            || self.timeout_ms == 0
            || self.timeout_ms > MAX_TEST_DURATION_MS
        {
            return invalid("plugin test case is invalid");
        }
        Ok(())
    }

    pub fn evaluate(&self, observation: &PluginTestObservation) -> Result<PluginTestVerdict> {
        self.validate()?;
        observation.validate()?;
        if self.case_id != observation.case_id {
            return invalid("plugin test observation belongs to another case");
        }

        let output_matched = observation.output_hash == self.expected_output_hash;
        let capability_scope_respected = observation
            .attempted_capabilities
            .is_subset(&self.allowed_capabilities);
        let completed_in_time = observation.duration_ms <= self.timeout_ms;
        Ok(PluginTestVerdict {
            case_id: self.case_id.clone(),
            passed: !observation.panicked
                && output_matched
                && capability_scope_respected
                && completed_in_time,
            output_matched,
            capability_scope_respected,
            completed_in_time,
            panicked: observation.panicked,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTestObservation {
    pub case_id: String,
    pub output_hash: [u8; 32],
    pub attempted_capabilities: BTreeSet<PluginCapability>,
    pub duration_ms: u64,
    pub panicked: bool,
}

impl PluginTestObservation {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.case_id)
            || self.output_hash == [0; 32]
            || self.duration_ms > MAX_TEST_DURATION_MS.saturating_mul(2)
        {
            return invalid("plugin test observation is invalid");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTestVerdict {
    pub case_id: String,
    pub passed: bool,
    pub output_matched: bool,
    pub capability_scope_respected: bool,
    pub completed_in_time: bool,
    pub panicked: bool,
}
