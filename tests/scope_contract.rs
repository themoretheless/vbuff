use vbuff_core::delivery::{ScopePolicy, ScopeSnapshot};

#[test]
fn current_workspace_stays_inside_the_mvp_crate_tripwire() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let workspace_crates = std::fs::read_dir(crate_root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .count();
    let tripwires = ScopePolicy::default().evaluate(ScopeSnapshot {
        workspace_crates: workspace_crates.try_into().unwrap(),
        added_mvp_milestones: 0,
        longest_open_milestone_days: 0,
    });
    assert!(
        !tripwires.crate_growth,
        "new MVP crate requires a re-scope review"
    );
    assert!(!tripwires.milestone_growth);
    assert!(!tripwires.milestone_stalled);
}
