#![cfg(feature = "elicitation")]

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use cordial::{
    CoverageTargetKind, ELICITATION_TRACKED_TARGETS, ElicitationTargetProvider, NamedRunFilter,
    SessionBuilder, TargetProvider, active_tracked_targets,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn tracked_targets_have_unique_upstream_and_shadow_names() {
    cordial::init_tracing();
    let mut upstream = HashSet::new();
    let mut shadow = HashSet::new();
    for target in ELICITATION_TRACKED_TARGETS {
        assert!(
            upstream.insert(target.upstream),
            "duplicate upstream {}",
            target.upstream
        );
        assert!(
            shadow.insert(target.shadow),
            "duplicate shadow {}",
            target.shadow
        );
    }
}

#[test]
fn active_tracked_targets_require_shadow_member() {
    cordial::init_tracing();
    let members: HashSet<String> = ["elicit_url", "elicitation"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let active = active_tracked_targets(&members);
    assert!(active.iter().any(|target| target.upstream == "url"));
    assert!(!active.iter().any(|target| target.upstream == "serde"));
}

#[test]
fn elicitation_provider_includes_shadow_pair_for_active_roster_entry() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace(
        fixture.path(),
        r#"
[workspace]
members = ["crates/elicitation", "crates/elicit_url"]
resolver = "2"
"#,
    )?;
    write_member(fixture.path(), "crates/elicitation", "elicitation")?;
    write_member(fixture.path(), "crates/elicit_url", "elicit_url")?;

    let session = SessionBuilder::new(fixture.path()).build();

    let targets = ElicitationTargetProvider
        .coverage_targets(&session, &NamedRunFilter::all_plugins())
        .into_diagnostic()
        .wrap_err("targets")?;
    assert!(
        targets.iter().any(|target| {
            target.kind == CoverageTargetKind::ShadowPair
                && target.crate_name == "url"
                && target.shadow_crate.as_deref() == Some("elicit_url")
        }),
        "expected url ↔ elicit_url shadow pair"
    );
    Ok(())
}

fn write_workspace(root: &Path, body: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.toml"), body)
        .into_diagnostic()
        .wrap_err("workspace manifest")?;
    Ok(())
}

fn write_member(root: &Path, rel: &str, name: &str) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}")
        .into_diagnostic()
        .wrap_err("lib rs")?;
    Ok(())
}
