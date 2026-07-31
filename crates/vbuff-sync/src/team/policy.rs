use std::collections::BTreeSet;

use super::{TeamDefaultDenylist, invalid, validate_id};
use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticPolicyCase {
    pub synthetic: bool,
    pub source_app_id: String,
    pub detector_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticPolicyDecision {
    pub denied: bool,
    pub reason: &'static str,
}

pub fn simulate_team_policy(
    policy: &TeamDefaultDenylist,
    case: &SyntheticPolicyCase,
) -> Result<SyntheticPolicyDecision> {
    if !case.synthetic {
        return invalid("team policy simulation accepts synthetic cases only");
    }
    validate_id(&case.source_app_id)?;
    if case.detector_ids.len() > 1_024 {
        return invalid("synthetic detector set is too large");
    }
    for detector_id in &case.detector_ids {
        validate_id(detector_id)?;
    }
    policy.validate()?;
    if policy.source_app_ids.contains(&case.source_app_id) {
        return Ok(SyntheticPolicyDecision {
            denied: true,
            reason: "source_app",
        });
    }
    if case
        .detector_ids
        .iter()
        .any(|detector| policy.detector_ids.contains(detector))
    {
        return Ok(SyntheticPolicyDecision {
            denied: true,
            reason: "detector",
        });
    }
    Ok(SyntheticPolicyDecision {
        denied: false,
        reason: "allowed",
    })
}
