//! Executable contracts for the large documentation set.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const BACKLOG_RANGES: [(&str, &str, usize, usize); 6] = [
    (
        "architecture.md",
        "## Ideas and improvements backlog",
        1,
        113,
    ),
    (
        "recommendation.md",
        "## Ideas and improvements backlog",
        114,
        197,
    ),
    ("docs/ideas-top-300.md", "## Power-User Workflows", 198, 300),
    (
        "docs/ideas-301-400.md",
        "## Privacy and Trust Controls",
        301,
        400,
    ),
    (
        "docs/ideas-401-500.md",
        "## Current Implementation Problems",
        401,
        500,
    ),
    (
        "docs/ideas-501-600.md",
        "## Native Clipboard Correctness",
        501,
        600,
    ),
];

const TOP_DOCS: [&str; 4] = [
    "README.md",
    "architecture.md",
    "recommendation.md",
    "plan.md",
];

#[test]
fn review_backlog_is_exactly_one_through_six_hundred() {
    let mut all = Vec::new();

    for (file, marker, start, end) in BACKLOG_RANGES {
        let source = read(file);
        let section = source
            .split_once(marker)
            .unwrap_or_else(|| panic!("{file} is missing section marker {marker:?}"))
            .1;
        let actual: Vec<usize> = section.lines().filter_map(numbered_backlog_item).collect();
        let expected: Vec<usize> = (start..=end).collect();
        assert_eq!(actual, expected, "wrong backlog range in {file}");
        all.extend(actual);
    }

    assert_eq!(all, (1..=600).collect::<Vec<_>>());
}

#[test]
fn top_docs_share_one_dry_backlog_map() {
    let map_rows = [
        "| 1-113 | [architecture.md](architecture.md) |",
        "| 114-197 | [recommendation.md](recommendation.md) |",
        "| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) |",
        "| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) |",
        "| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) |",
        "| 501-600 | [docs/ideas-501-600.md](docs/ideas-501-600.md) |",
    ];

    for file in TOP_DOCS {
        let source = read(file);
        for row in map_rows {
            assert!(
                source.contains(row),
                "{file} is missing shared map row {row}"
            );
        }
    }
}

#[test]
fn top_docs_link_every_complete_implementation_batch() {
    for (ledger_link, expected) in [
        (
            "docs/implementation-batch-001-050.md",
            (1..=50).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-051-100.md",
            (51..=100).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-101-150.md",
            (101..=150).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-151-200.md",
            (151..=200).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-201-250.md",
            (201..=250).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-251-300.md",
            (251..=300).collect::<Vec<_>>(),
        ),
        (
            "docs/implementation-batch-301-350.md",
            (301..=350).collect::<Vec<_>>(),
        ),
    ] {
        for file in TOP_DOCS {
            assert!(
                read(file).contains(ledger_link),
                "{file} does not link implementation ledger {ledger_link}"
            );
        }

        let ledger = read(ledger_link);
        let item_section = ledger
            .split_once("## Item ledger")
            .expect("batch ledger is missing its item section")
            .1
            .split_once("## Three review passes")
            .expect("batch ledger is missing its review section")
            .0;
        let items = item_section
            .lines()
            .filter_map(|line| {
                let mut columns = line.split('|');
                let _leading = columns.next()?;
                columns.next()?.trim().parse::<usize>().ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(items, expected);
    }
}

#[test]
fn research_catalog_has_exact_repository_and_source_ids() {
    let research = read("docs/repositories-research-100.md");
    assert!(research.contains("verified through the GitHub GraphQL API on 2026-07-18"));
    let repository_rows: Vec<&str> = research
        .lines()
        .filter(|line| numbered_table_id(line, "GH-").is_some())
        .collect();
    let repositories: Vec<usize> = repository_rows
        .iter()
        .filter_map(|line| numbered_table_id(line, "GH-"))
        .collect();
    let sources: Vec<usize> = research
        .lines()
        .filter_map(|line| numbered_table_id(line, "S-"))
        .collect();

    assert_eq!(repositories, (1..=100).collect::<Vec<_>>());
    assert_eq!(sources, (1..=26).collect::<Vec<_>>());

    let mut repository_links = HashSet::new();
    for row in repository_rows {
        let columns: Vec<&str> = row.split('|').collect();
        let stars: usize = columns[3]
            .trim()
            .replace(',', "")
            .parse()
            .unwrap_or_else(|error| panic!("invalid star count in {row}: {error}"));
        assert!(stars >= 500, "repository is below the catalog floor: {row}");

        let link = columns[2]
            .split_once("](https://github.com/")
            .and_then(|(_, target)| target.trim().strip_suffix(')'))
            .unwrap_or_else(|| panic!("invalid GitHub link in {row}"));
        assert!(
            repository_links.insert(link),
            "duplicate repository in research catalog: {link}"
        );
    }

    let ideas = read("docs/ideas-501-600.md");
    let evidenced_items: Vec<&str> = ideas
        .lines()
        .filter(|line| numbered_backlog_item(line).is_some())
        .filter(|line| line.contains("Evidence:"))
        .collect();
    assert_eq!(evidenced_items.len(), 100);

    for line in evidenced_items {
        assert_evidence_ids(line);
    }

    for (file, expected) in [
        ("docs/ideas-601-610.md", 601..=610),
        ("docs/ideas-611-620.md", 611..=620),
    ] {
        let candidate_tail = read(file);
        let candidate_lines = candidate_tail
            .lines()
            .filter(|line| numbered_backlog_item(line).is_some())
            .collect::<Vec<_>>();
        assert_eq!(
            candidate_lines
                .iter()
                .filter_map(|line| numbered_backlog_item(line))
                .collect::<Vec<_>>(),
            expected.collect::<Vec<_>>()
        );
        assert_eq!(candidate_lines.len(), 10);
        for line in candidate_lines {
            assert!(line.contains("Evidence:"));
            assert_evidence_ids(line);
        }
    }
}

#[test]
fn solid_dry_design_and_scope_sections_stay_visible() {
    let readme = read("README.md");
    assert!(readme.contains("## Read the project in small pieces"));
    assert!(readme.contains("## Design direction"));
    assert!(readme.contains("`AppCommand` is the one command vocabulary"));
    assert!(readme.contains("typed capture-health vocabulary"));
    assert!(readme.contains("read `src/diagnostics.rs`"));
    assert!(readme.contains("read `src/single_instance/mod.rs`"));
    assert!(readme.contains("duplicate launch forwards `ShowPopup`"));
    assert!(readme.contains("Twenty-eight checked-in golden images"));
    assert!(readme.contains("docs/decision-gates-151-200.md"));
    assert!(readme.contains("docs/decision-gates-201-250.md"));
    assert!(readme.contains("docs/decision-gates-251-300.md"));
    assert!(readme.contains("docs/decision-gates-301-350.md"));
    assert!(readme.contains("docs/limitations.md"));
    assert!(readme.contains("docs/maintainer-handoff.md"));
    assert!(readme.contains("docs/scope-review.md"));
    assert!(readme.contains("docs/data-contract-v1.md"));
    assert!(readme.contains("docs/data-contract-v2.md"));
    assert!(readme.contains("docs/data-contract-v3.md"));
    assert!(readme.contains("docs/ideas-601-610.md"));
    assert!(readme.contains("docs/ideas-611-620.md"));

    let architecture = read("architecture.md");
    assert!(architecture.contains("### SOLID/DRY decomposition and small reading slices"));
    assert!(architecture.contains("| `src/capture.rs` |"));
    assert!(architecture.contains("| `src/paste.rs` |"));
    assert!(architecture.contains("the narrow `Diagnostics` publisher"));
    assert!(architecture.contains("`crates/vbuff-types/src/status.rs`"));
    assert!(architecture.contains("| `src/single_instance/` |"));
    assert!(architecture.contains("`CaptureHealth::Stalled`"));
    assert!(architecture.contains("schema v7"));
    assert!(architecture.contains("History/Stack/Privacy/Settings"));
    assert!(architecture.contains("workflow/everyday.rs"));
    assert!(architecture.contains("device_experience.rs"));
    assert!(architecture.contains("data_lifecycle.rs"));
    assert!(architecture.contains("`trust/`"));
    assert!(architecture.contains("`recall/`"));

    let recommendation = read("recommendation.md");
    assert!(recommendation.contains("### Design direction and product cut line"));
    assert!(recommendation.contains("The SOLID/DRY product rule"));
    assert!(recommendation.contains("one typed capture-health vocabulary"));
    assert!(recommendation.contains("pause-aware heartbeat/watchdog"));
    assert!(recommendation.contains("batch 151-200"));
    assert!(recommendation.contains("batch 201-250"));
    assert!(recommendation.contains("batch 251-300"));
    assert!(recommendation.contains("batch 301-350"));

    let plan = read("plan.md");
    assert!(plan.contains("not an implicit scope increase"));
    assert!(plan.contains("Current baseline before the formal M7 crate extraction"));
    assert!(plan.contains("Serializable status contracts live in `vbuff-types`"));
    assert!(plan.contains("Bootstrap already landed in the root app"));
    assert!(plan.contains("native hook re-subscribe/auto-restart"));
    assert!(plan.contains("M6 -> M7 data-contract gate"));
    assert!(plan.contains("Unknown` is a release blocker"));
    assert!(plan.contains("docs/ideas-601-610.md"));
    assert!(plan.contains("docs/ideas-611-620.md"));
    assert!(plan.contains("docs/decision-gates-251-300.md"));
    assert!(plan.contains("docs/decision-gates-301-350.md"));
    assert!(plan.contains("docs/data-contract-v3.md"));
}

#[test]
fn operations_documents_keep_release_and_scope_claims_honest() {
    let limitations = read("docs/limitations.md");
    let limitation_ids = limitations
        .lines()
        .filter_map(|line| numbered_table_id(line, "LIM-"))
        .collect::<Vec<_>>();
    assert_eq!(limitation_ids, (1..=13).collect::<Vec<_>>());
    assert!(limitations.contains("not encrypted at rest"));
    assert!(
        limitations
            .to_ascii_lowercase()
            .contains("whole-database encryption")
    );

    let release = read(".github/workflows/release-provenance.yml");
    for evidence in [
        "tests-${{ matrix.name }}.log",
        "canary-scope.txt",
        "cargo-deny.log",
        "cargo-audit.log",
        "cargo-vet.log",
        "performance-core.log",
        "performance-store.log",
        "cargo cyclonedx",
        "MANIFEST.sha256",
    ] {
        assert!(
            release.contains(evidence),
            "release evidence omits {evidence}"
        );
    }

    let handoff = read("docs/maintainer-handoff.md");
    for section in [
        "## Access inventory",
        "## Normal release",
        "## Emergency patch",
        "## Dependency cadence",
        "## Sunset policy",
        "## Handoff drill",
    ] {
        assert!(handoff.contains(section), "handoff omits {section}");
    }

    let scope = read("docs/scope-review.md");
    for disposition in ["**Promote**", "**Keep**", "**Defer**", "**Cut**"] {
        assert!(scope.contains(disposition));
    }
    let reminder = read(".github/workflows/quarterly-scope-review.yml");
    assert!(reminder.contains("1,4,7,10"));
    assert!(reminder.contains("gh issue create"));
}

#[test]
fn local_markdown_links_resolve() {
    let docs = [
        "README.md",
        "architecture.md",
        "recommendation.md",
        "plan.md",
        "docs/ideas-top-300.md",
        "docs/ideas-301-400.md",
        "docs/ideas-401-500.md",
        "docs/ideas-501-600.md",
        "docs/ideas-601-610.md",
        "docs/repositories-research-100.md",
        "docs/implementation-batch-001-050.md",
        "docs/implementation-batch-051-100.md",
        "docs/implementation-batch-101-150.md",
        "docs/implementation-batch-151-200.md",
        "docs/implementation-batch-201-250.md",
        "docs/implementation-batch-251-300.md",
        "docs/decision-gates-151-200.md",
        "docs/decision-gates-201-250.md",
        "docs/decision-gates-251-300.md",
        "docs/data-contract-v1.md",
        "docs/data-contract-v2.md",
        "docs/product-strategy-decisions.md",
        "docs/limitations.md",
        "docs/maintainer-handoff.md",
        "docs/scope-review.md",
    ];

    for file in docs {
        let source = read(file);
        for target in markdown_link_targets(&source) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
                || target.starts_with("mailto:")
            {
                continue;
            }

            let target = target.split('#').next().unwrap_or_default();
            if target.is_empty() {
                continue;
            }
            let resolved = workspace_root()
                .join(Path::new(file).parent().unwrap_or_else(|| Path::new("")))
                .join(target);
            assert!(
                resolved.exists(),
                "broken local link in {file}: {target} resolves to {}",
                resolved.display()
            );
        }
    }
}

fn numbered_backlog_item(line: &str) -> Option<usize> {
    let (number, rest) = line.split_once(". ")?;
    rest.starts_with("**")
        .then(|| number.parse().ok())
        .flatten()
}

fn numbered_table_id(line: &str, prefix: &str) -> Option<usize> {
    let value = line.strip_prefix("| ")?.strip_prefix(prefix)?;
    let (number, _) = value.split_once(" |")?;
    number.parse().ok()
}

fn assert_evidence_ids(line: &str) {
    let evidence_ids = line
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .filter(|token| token.starts_with("GH-") || token.starts_with("S-"))
        .collect::<Vec<_>>();
    assert!(
        !evidence_ids.is_empty(),
        "backlog item has no evidence id: {line}"
    );
    for id in evidence_ids {
        let (prefix, number) = id
            .split_once('-')
            .unwrap_or_else(|| panic!("invalid evidence id {id}"));
        let number: usize = number
            .parse()
            .unwrap_or_else(|error| panic!("invalid evidence id {id}: {error}"));
        match prefix {
            "GH" => assert!((1..=100).contains(&number), "unknown evidence id {id}"),
            "S" => assert!((1..=26).contains(&number), "unknown evidence id {id}"),
            _ => unreachable!(),
        }
    }
}

fn markdown_link_targets(source: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut remaining = source;
    while let Some((_, after_open)) = remaining.split_once("](") {
        let Some((target, after_close)) = after_open.split_once(')') else {
            break;
        };
        targets.push(target);
        remaining = after_close;
    }
    targets
}

fn read(relative: &str) -> String {
    fs::read_to_string(workspace_root().join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
