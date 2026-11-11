//! The dependency-freshness survey: what it reports from manifests and lockfiles, and the
//! artifact the etiquette writes.

use cordial::{DEPENDENCY_FRESHNESS_ETIQUETTE, NamedRunFilter, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/dependency_freshness_workspace.rs"]
mod dependency_freshness_workspace;

use dependency_freshness_workspace::{
    dependency_freshness_survey_csv, write_dependency_freshness_cache, write_lockfile,
    write_member_manifest, write_package_manifest, write_workspace_manifest,
};

#[test]
fn dependency_freshness_survey_reports_manifest_and_lockfile_indicators() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_workspace_manifest(
        fixture.path(),
        r#"
[workspace]
members = [
    "crates/app",
    "crates/actual-package",
    "crates/nix",
    "crates/regex",
    "crates/serde",
    "crates/serde-json",
    "crates/tempfile",
]
resolver = "2"

[workspace.dependencies]
serde_json = { path = "crates/serde-json", version = "1" }
actual = { package = "actual-package", path = "crates/actual-package", version = "=2.3.4" }
"#,
    )?;
    write_package_manifest(
        fixture.path(),
        "crates/actual-package",
        "actual-package",
        "2.3.4",
    )?;
    write_package_manifest(fixture.path(), "crates/nix", "nix", "0.28.0")?;
    write_package_manifest(fixture.path(), "crates/regex", "regex", "1.10.0")?;
    write_package_manifest(fixture.path(), "crates/serde", "serde", "1.0.228")?;
    write_package_manifest(fixture.path(), "crates/serde-json", "serde_json", "1.0.145")?;
    write_package_manifest(fixture.path(), "crates/tempfile", "tempfile", "3.0.0")?;
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
serde_json = { workspace = true }
regex = { version = "~1.10", path = "../regex", default-features = false }
actual = { package = "actual-package", workspace = true }

[dev-dependencies]
tempfile = { version = "*", path = "../tempfile" }

[target.'cfg(unix)'.dependencies]
nix = { version = ">=0.28, <0.30", path = "../nix" }
"#,
    )?;
    write_lockfile(
        fixture.path(),
        r#"
version = 4

[[package]]
name = "serde"
version = "1.0.228"

[[package]]
name = "serde_json"
version = "1.0.145"

[[package]]
name = "actual-package"
version = "2.3.4"
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
        .run(&NamedRunFilter::all_etiquettes().with_tiers(["deep"]))
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 4);

    let csv = dependency_freshness_survey_csv(store.path())?;
    assert!(csv.contains("app,serde,serde"));
    assert!(csv.contains("1.0.228"));
    assert!(csv.contains("lockfile_resolved"));
    assert!(csv.contains("app,serde_json,serde_json"));
    assert!(csv.contains("workspace = true (1)"));
    assert!(csv.contains("manifest_workspace_inherited"));
    assert!(csv.contains("app,actual,actual-package"));
    assert!(csv.contains("exact_pin"));
    assert!(csv.contains("app,tempfile,tempfile"));
    assert!(csv.contains("wildcard"));
    assert!(csv.contains("app,nix,nix"));
    assert!(csv.contains("upper_bound"));

    let findings = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness.csv"),
    )
    .into_diagnostic()
    .wrap_err("dependency freshness csv")?;
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN"));
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND"));
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD"));
    assert!(findings.contains("DEPENDENCY-FRESHNESS-MANIFEST-TILDE"));
    assert!(findings.contains("uses exact manifest requirement"));
    assert!(findings.contains("uses upper-bounded manifest requirement"));
    assert!(findings.contains("uses wildcard manifest requirement"));
    assert!(findings.contains("uses tilde manifest requirement"));
    Ok(())
}

#[test]
fn dependency_freshness_etiquette_writes_survey_artifact() -> miette::Result<()> {
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
    write_lockfile(
        fixture.path(),
        r#"
version = 4

[[package]]
name = "app"
version = "0.1.0"
dependencies = [
 "local_dep",
]

[[package]]
name = "local_dep"
version = "0.1.0"
"#,
    )?;

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

    let csv = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("dependency-freshness-survey.csv"),
    )
    .into_diagnostic()
    .wrap_err("survey csv")?;
    assert!(csv.contains("app,local_dep,local_dep"));
    assert!(csv.contains("path"));
    assert!(csv.contains("lockfile_resolved"));
    Ok(())
}
