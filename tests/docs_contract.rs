//! Executable contracts for the large documentation set.

use std::fs;
use std::path::{Path, PathBuf};

const BACKLOG_RANGES: [(&str, &str, usize, usize); 5] = [
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
];

const TOP_DOCS: [&str; 3] = ["README.md", "architecture.md", "recommendation.md"];

#[test]
fn review_backlog_is_exactly_one_through_five_hundred() {
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

    assert_eq!(all, (1..=500).collect::<Vec<_>>());
}

#[test]
fn top_docs_share_one_dry_backlog_map() {
    let map_rows = [
        "| 1-113 | [architecture.md](architecture.md) |",
        "| 114-197 | [recommendation.md](recommendation.md) |",
        "| 198-300 | [docs/ideas-top-300.md](docs/ideas-top-300.md) |",
        "| 301-400 | [docs/ideas-301-400.md](docs/ideas-301-400.md) |",
        "| 401-500 | [docs/ideas-401-500.md](docs/ideas-401-500.md) |",
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
fn solid_dry_design_and_scope_sections_stay_visible() {
    let readme = read("README.md");
    assert!(readme.contains("## Read the project in small pieces"));
    assert!(readme.contains("## Design direction"));
    assert!(readme.contains("`AppCommand` is the one command vocabulary"));
    assert!(readme.contains("typed capture-health state"));
    assert!(readme.contains("read `src/diagnostics.rs`"));
    assert!(readme.contains("read `src/single_instance/mod.rs`"));
    assert!(readme.contains("duplicate launch forwards `ShowPopup`"));

    let architecture = read("architecture.md");
    assert!(architecture.contains("### SOLID/DRY decomposition and small reading slices"));
    assert!(architecture.contains("| `src/capture.rs` |"));
    assert!(architecture.contains("| `src/paste.rs` |"));
    assert!(architecture.contains("the narrow `Diagnostics` publisher"));
    assert!(architecture.contains("`crates/vbuff-types/src/status.rs`"));
    assert!(architecture.contains("| `src/single_instance/` |"));
    assert!(architecture.contains("`CaptureHealth::Stalled`"));

    let recommendation = read("recommendation.md");
    assert!(recommendation.contains("### Design direction and product cut line"));
    assert!(recommendation.contains("The SOLID/DRY product rule"));
    assert!(recommendation.contains("one typed capture-health vocabulary"));
    assert!(recommendation.contains("pause-aware heartbeat/watchdog"));

    let plan = read("plan.md");
    assert!(plan.contains("not an implicit scope increase"));
    assert!(plan.contains("Current baseline before the formal M7 crate extraction"));
    assert!(plan.contains("Serializable status contracts live in `vbuff-types`"));
    assert!(plan.contains("Bootstrap already landed in the root app"));
    assert!(plan.contains("native hook re-subscribe/auto-restart"));
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
