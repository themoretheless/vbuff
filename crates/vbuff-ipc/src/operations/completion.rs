use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{MAX_COMPLETION_RESULTS, OperationContractError, OperationResult, validate_id};

const MAX_COMPLETIONS_PER_KIND: usize = 2_048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    Tag,
    Collection,
    ContentKind,
    Device,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionCandidate {
    pub kind: CompletionKind,
    pub value: String,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCompletionCatalog {
    values: BTreeMap<CompletionKind, BTreeSet<String>>,
}

impl std::fmt::Debug for ShellCompletionCatalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellCompletionCatalog")
            .field("kind_count", &self.values.len())
            .field(
                "value_count",
                &self.values.values().map(BTreeSet::len).sum::<usize>(),
            )
            .finish()
    }
}

impl ShellCompletionCatalog {
    pub fn insert(&mut self, kind: CompletionKind, value: String) -> OperationResult<()> {
        validate_id(&value)?;
        let values = self.values.entry(kind).or_default();
        if !values.contains(&value) && values.len() >= MAX_COMPLETIONS_PER_KIND {
            return Err(OperationContractError::Invalid("completion_catalog_full"));
        }
        values.insert(value);
        Ok(())
    }

    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<CompletionCandidate> {
        let limit = limit.min(MAX_COMPLETION_RESULTS);
        self.values
            .iter()
            .flat_map(|(kind, values)| {
                values
                    .iter()
                    .filter(move |value| value.starts_with(prefix))
                    .map(move |value| CompletionCandidate {
                        kind: *kind,
                        value: value.clone(),
                    })
            })
            .take(limit)
            .collect()
    }
}
