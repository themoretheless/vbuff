use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{SharedVariableCatalog, invalid, validate_id};
use crate::Result;

const MAX_IMPORT_ITEMS: usize = 512;
const MAX_IMPORT_VARIABLES: usize = 128;
const MAX_IMPORT_ACTIONS: usize = 64;
const MAX_ALLOWED_ACTIONS: usize = 256;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSnippetImport {
    pub snippet_id: String,
    pub template: String,
    pub variables: BTreeSet<String>,
    pub action_ids: BTreeSet<String>,
}

impl std::fmt::Debug for TeamSnippetImport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TeamSnippetImport")
            .field("snippet_id", &self.snippet_id)
            .field("template", &"[redacted]")
            .field("variables", &self.variables)
            .field("action_ids", &self.action_ids)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamImportValidation {
    pub accepted_ids: Vec<String>,
    pub rejected: BTreeMap<String, String>,
}

pub fn validate_team_import(
    candidates: &[TeamSnippetImport],
    variables: &SharedVariableCatalog,
    allowed_actions: &BTreeSet<String>,
) -> TeamImportValidation {
    let mut validation = TeamImportValidation {
        accepted_ids: Vec::new(),
        rejected: BTreeMap::new(),
    };
    if candidates.len() > MAX_IMPORT_ITEMS {
        validation
            .rejected
            .insert("batch".into(), "too_many_import_items".into());
        return validation;
    }
    if allowed_actions.len() > MAX_ALLOWED_ACTIONS
        || allowed_actions
            .iter()
            .any(|action_id| validate_id(action_id).is_err())
    {
        validation
            .rejected
            .insert("batch".into(), "invalid_action_allowlist".into());
        return validation;
    }
    let mut id_counts = BTreeMap::new();
    for candidate in candidates {
        *id_counts
            .entry(candidate.snippet_id.as_str())
            .or_insert(0usize) += 1;
    }
    for candidate in candidates {
        let reason = if id_counts
            .get(candidate.snippet_id.as_str())
            .is_some_and(|count| *count > 1)
        {
            Some("duplicate_snippet_id")
        } else if validate_id(&candidate.snippet_id).is_err()
            || candidate.template.is_empty()
            || candidate.template.len() > 64 * 1024
            || candidate.template.contains('\0')
        {
            Some("invalid_snippet")
        } else if candidate.variables.len() > MAX_IMPORT_VARIABLES
            || candidate.action_ids.len() > MAX_IMPORT_ACTIONS
            || candidate
                .variables
                .iter()
                .chain(&candidate.action_ids)
                .any(|value| validate_id(value).is_err())
        {
            Some("invalid_snippet_scope")
        } else if candidate
            .variables
            .iter()
            .any(|name| !variables.contains(name))
        {
            Some("unknown_variable")
        } else if !candidate.action_ids.is_subset(allowed_actions) {
            Some("unsafe_action")
        } else {
            None
        };
        if let Some(reason) = reason {
            validation
                .rejected
                .insert(candidate.snippet_id.clone(), reason.into());
        } else {
            validation.accepted_ids.push(candidate.snippet_id.clone());
        }
    }
    validation
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedTeamPluginApproval {
    pub team_id: String,
    pub plugin_id: String,
    pub manifest_hash: [u8; 32],
    pub collection_ids: BTreeSet<String>,
    pub capability_ids: BTreeSet<String>,
}

impl ScopedTeamPluginApproval {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.team_id)?;
        validate_id(&self.plugin_id)?;
        if self.manifest_hash == [0; 32]
            || self.collection_ids.is_empty()
            || self.collection_ids.len() > 64
            || self.capability_ids.is_empty()
            || self.capability_ids.len() > 32
        {
            return invalid("plugin approval must have bounded explicit scope");
        }
        for value in self.collection_ids.iter().chain(&self.capability_ids) {
            validate_id(value)?;
        }
        Ok(())
    }

    pub fn allows(
        &self,
        team_id: &str,
        manifest_hash: &[u8; 32],
        collection_id: &str,
        capability_id: &str,
    ) -> bool {
        self.validate().is_ok()
            && self.team_id == team_id
            && &self.manifest_hash == manifest_hash
            && self.collection_ids.contains(collection_id)
            && self.capability_ids.contains(capability_id)
    }
}
