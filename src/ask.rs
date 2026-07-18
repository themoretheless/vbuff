//! Local-first natural-language-shaped history query command.

use std::fmt;

use serde::Serialize;
use vbuff_store::Store;

const DEFAULT_LIMIT: usize = 10;
const MAX_QUERY_BYTES: usize = 4_096;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AskCommand {
    query: String,
    limit: usize,
    json: bool,
}

impl fmt::Debug for AskCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AskCommand")
            .field(
                "query",
                &format_args!("[redacted; {} bytes]", self.query.len()),
            )
            .field("limit", &self.limit)
            .field("json", &self.json)
            .finish()
    }
}

#[derive(Serialize)]
struct AskOutput {
    engine: &'static str,
    semantic_model: bool,
    results: Vec<AskResult>,
}

#[derive(Serialize)]
struct AskResult {
    id: String,
    kind: &'static str,
    preview: String,
}

pub(crate) fn requested() -> anyhow::Result<Option<AskCommand>> {
    parse_requested(std::env::args().skip(1))
}

fn parse_requested(
    arguments: impl IntoIterator<Item = String>,
) -> anyhow::Result<Option<AskCommand>> {
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("ask") {
        return Ok(None);
    }
    let mut json = false;
    let mut limit = DEFAULT_LIMIT;
    let mut query = Vec::new();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--json" => json = true,
            "--limit" => {
                limit = arguments
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--limit requires a value"))?
                    .parse()?;
            }
            _ if argument.starts_with('-') => anyhow::bail!("unknown ask option: {argument}"),
            _ => query.push(argument),
        }
    }
    let query = query.join(" ");
    anyhow::ensure!(
        !query.trim().is_empty() && query.len() <= MAX_QUERY_BYTES,
        "usage: vbuff ask [--json] [--limit N] <query>"
    );
    anyhow::ensure!(
        (1..=512).contains(&limit),
        "ask limit must be between 1 and 512"
    );
    Ok(Some(AskCommand { query, limit, json }))
}

pub(crate) fn run(command: AskCommand) -> anyhow::Result<()> {
    let store = Store::open_default()?;
    let _ = store.backfill_embeddings(command.limit.saturating_mul(8).min(512))?;
    let clips = store.local_similarity_search(&command.query, command.limit)?;
    let output = AskOutput {
        engine: "local-feature-hash-v1",
        semantic_model: false,
        results: clips
            .into_iter()
            .map(|clip| AskResult {
                id: clip.id.to_string_repr(),
                kind: clip.meta.kind.label(),
                preview: clip.preview(240),
            })
            .collect(),
    };
    if command.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if output.results.is_empty() {
        println!("No AI-eligible local matches");
    } else {
        for result in output.results {
            println!("{}\t{}\t{}", result.id, result.kind, result.preview);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_parser_is_bounded_and_keeps_query_words() {
        assert_eq!(parse_requested(["doctor".into()]).unwrap(), None);
        assert_eq!(
            parse_requested([
                "ask".into(),
                "--json".into(),
                "--limit".into(),
                "3".into(),
                "meeting".into(),
                "link".into(),
            ])
            .unwrap(),
            Some(AskCommand {
                query: "meeting link".into(),
                limit: 3,
                json: true,
            })
        );
        assert!(parse_requested(["ask".into(), "--limit".into(), "0".into(), "x".into()]).is_err());
        let command = parse_requested(["ask".into(), "private".into(), "diagnosis".into()])
            .unwrap()
            .unwrap();
        assert!(!format!("{command:?}").contains("diagnosis"));
    }
}
