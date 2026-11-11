//! Dependency-freshness findings: workspace policy bypass, cache overrides, and the config gate.

use cordial::{
    DEPENDENCY_FRESHNESS_ETIQUETTE, NamedRunFilter, Session, SessionBuilder, load_cordial_config,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/dependency_freshness_workspace.rs"]
mod dependency_freshness_workspace;

use dependency_freshness_workspace::{
    dependency_freshness_survey_csv, write_dependency_freshness_cache, write_lockfile,
    write_member_manifest, write_package_manifest, write_workspace_manifest,
};

#[test]
fn dependency_freshness_lints_workspace_policy_bypass() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace_manifest(
        fixture.path(),
        r#"
[workspace]
members = ["crates/app", "crates/serde"]
resolver = "2"

[workspace.dependencies]
serde = { path = "crates/serde", version = "1" }
"#,
    )?;
    write_package_manifest(fixture.path(), "crates/serde", "serde", "1.0.0")?;
    write_member_manifest(
        fixture.path(),
        "crates/app",
        r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", path = "../serde" }
"#,
    )?;
    write_lockfile(
        fixture.path(),
        r#"
version = 4

[[package]]
name = "app"
version = "0.1.0"
dependencies = [
 "serde",
]

[[package]]
name = "serde"
version = "1.0.0"
"#,
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    write_dependency_freshness_cache(store.path(), "")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(
            &NamedRunFilter::all_etiquettes()
                .with_crate("app")
                .with_tiers(["deep"]),
        )
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 1);

    let survey = dependency_freshness_survey_csv(store.path())?;
    assert!(survey.contains("app,serde,serde"));
    assert!(survey.contains("manifest_workspace_bypass"));

    let findings = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness.csv"),
    )
    .into_diagnostic()
    .wrap_err("dependency freshness csv")?;
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS"));
    assert!(findings.contains("declares local dependency policy instead of workspace = true"));
    Ok(())
}

#[test]
fn dependency_freshness_cache_override_drives_open_findings() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace_manifest(
        fixture.path(),
        r#"
[workspace]
members = [
    "crates/anyhow",
    "crates/app",
    "crates/regex",
    "crates/serde",
    "crates/syn",
]
resolver = "2"

[workspace.dependencies]
anyhow = { path = "crates/anyhow", version = "1" }
regex = { path = "crates/regex", version = "1" }
serde = { path = "crates/serde", version = "1" }
syn = { path = "crates/syn", version = "2" }
"#,
    )?;
    write_package_manifest(fixture.path(), "crates/anyhow", "anyhow", "1.0.0")?;
    write_package_manifest(fixture.path(), "crates/regex", "regex", "1.10.0")?;
    write_package_manifest(fixture.path(), "crates/serde", "serde", "1.0.220")?;
    write_package_manifest(fixture.path(), "crates/syn", "syn", "2.0.119")?;
    write_member_manifest(
        fixture.path(),
        "crates/app",
        r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { workspace = true }
regex = { workspace = true }
anyhow = { workspace = true }
syn = { workspace = true }
"#,
    )?;
    write_lockfile(
        fixture.path(),
        r#"
version = 4

[[package]]
name = "serde"
version = "1.0.220"

[[package]]
name = "regex"
version = "1.10.0"

[[package]]
name = "anyhow"
version = "1.0.0"

[[package]]
name = "syn"
version = "2.0.119"

[[package]]
name = "syn"
version = "3.0.3"
"#,
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    write_dependency_freshness_cache(
        store.path(),
        r#"
[[package]]
name = "serde"
available_version = "1.0.228"

[[package]]
name = "regex"
available_version = "1.11.0"

[[package]]
name = "anyhow"
available_version = "2.0.0"

[[package]]
name = "syn"
available_version = "3.0.5"
"#,
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[dependency_freshness]\nminor = false\n",
    )
    .into_diagnostic()
    .wrap_err("cordial.toml")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(
            &NamedRunFilter::all_etiquettes()
                .with_crate("app")
                .with_tiers(["deep"]),
        )
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 3);

    let findings = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness.csv"),
    )
    .into_diagnostic()
    .wrap_err("dependency freshness csv")?;
    assert!(findings.contains("DEPENDENCY-FRESHNESS-PATCH"));
    assert!(!findings.contains("DEPENDENCY-FRESHNESS-MINOR"));
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MAJOR"));
    assert!(findings.contains("syn"));
    assert!(findings.contains("2.0.119"));
    assert!(!findings.contains("2.0.119|3.0.3"));
    assert!(findings.contains("serde"));
    assert!(findings.contains("1.0.220"));
    assert!(findings.contains("1.0.228"));

    let survey = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness-survey.csv"),
    )
    .into_diagnostic()
    .wrap_err("survey csv")?;
    assert!(survey.contains("available_versions,update_kinds,rule_ids"));
    assert!(survey.contains("patch_available"));
    assert!(survey.contains("minor_available"));
    assert!(survey.contains("major_available"));

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 3"));
    Ok(())
}

#[test]
fn dependency_freshness_config_gate_disables_etiquette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace_manifest(
        fixture.path(),
        r#"
[workspace]
members = ["crates/app", "crates/local_dep"]
resolver = "2"
"#,
    )?;
    write_member_manifest(
        fixture.path(),
        "crates/local_dep",
        r#"
[package]
name = "local_dep"
version = "0.1.0"
edition = "2024"
"#,
    )?;
    write_member_manifest(
        fixture.path(),
        "crates/app",
        r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
local_dep = { path = "../local_dep" }
"#,
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[dependency_freshness]\nenabled = false\n",
    )
    .into_diagnostic()
    .wrap_err("cordial.toml")?;

    let config = load_cordial_config(fixture.path(), fixture.path());
    assert!(!config.etiquette_enabled("dependency_freshness"));

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(
            &NamedRunFilter::all_etiquettes()
                .with_crate("app")
                .with_tiers(["deep"]),
        )
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 0);
    assert_eq!(outcome.artifacts().count(), 0);
    assert!(
        !store
            .path()
            .join("findings")
            .join("dependency-freshness-survey.csv")
            .is_file()
    );
    Ok(())
}
