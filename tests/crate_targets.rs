use std::fs;
use std::path::Path;

use cordial::{NamedRunFilter, discover_crate_targets, project_slug_from_path};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn non_cargo_project_uses_directory_slug() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let targets = discover_crate_targets(fixture.path(), &NamedRunFilter::all_etiquettes())
        .into_diagnostic()
        .wrap_err("targets")?;
    assert_eq!(targets.len(), 1);
    assert_eq!(
        targets[0].crate_name,
        project_slug_from_path(fixture.path())
    );
    Ok(())
}

#[test]
fn workspace_members_discovered_from_manifest() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace(
        fixture.path(),
        r#"
[workspace]
members = ["crates/alpha", "crates/beta"]
resolver = "2"
"#,
    )?;
    write_member_crate(fixture.path(), "crates/alpha", "alpha")?;
    write_member_crate(fixture.path(), "crates/beta", "beta")?;

    let targets = discover_crate_targets(fixture.path(), &NamedRunFilter::all_etiquettes())
        .into_diagnostic()
        .wrap_err("targets")?;
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().any(|target| target.crate_name == "alpha"));
    assert!(targets.iter().any(|target| target.crate_name == "beta"));
    Ok(())
}

#[test]
fn crate_name_filter_selects_one_member() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace(
        fixture.path(),
        r#"
[workspace]
members = ["crates/alpha", "crates/beta"]
resolver = "2"
"#,
    )?;
    write_member_crate(fixture.path(), "crates/alpha", "alpha")?;
    write_member_crate(fixture.path(), "crates/beta", "beta")?;

    let filter = NamedRunFilter::all_etiquettes().with_crate("beta");
    let targets = discover_crate_targets(fixture.path(), &filter)
        .into_diagnostic()
        .wrap_err("targets")?;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name, "beta");
    Ok(())
}

#[test]
#[cfg(feature = "elicitation")]
fn coverage_plugin_shadow_pair_includes_both_crates_when_filtered_to_upstream() -> miette::Result<()>
{
    use std::collections::HashSet;
    use std::path::PathBuf;

    use cordial::{ELICITATION_COVERAGE, Plugin, discover_run_crate_targets};

    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/minimal-workspace");
    let session = cordial::SessionBuilder::new(&fixture).build();
    let filter = NamedRunFilter::all_plugins().with_crate("url".to_string());
    let plugins = vec![&ELICITATION_COVERAGE as &dyn Plugin];

    let targets = discover_run_crate_targets(&plugins, &fixture, &session, &filter)
        .into_diagnostic()
        .wrap_err("targets")?;
    let names: HashSet<String> = targets.into_iter().map(|t| t.crate_name).collect();
    assert!(names.contains("url"));
    assert!(
        names.contains("elicit_url"),
        "shadow pair should schedule shadow crate IR when filtering upstream"
    );
    Ok(())
}

fn write_workspace(root: &Path, body: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.toml"), body)
        .into_diagnostic()
        .wrap_err("workspace manifest")?;
    Ok(())
}

fn write_member_crate(root: &Path, rel: &str, name: &str) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
"#
        ),
    )
    .into_diagnostic()
    .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}")
        .into_diagnostic()
        .wrap_err("lib rs")?;
    Ok(())
}
