use std::fs;
use std::path::Path;

use cordial::{
    DEPENDENCY_FRESHNESS_ETIQUETTE, NamedRunFilter, Session, SessionBuilder, load_cordial_config,
};
use miette::{IntoDiagnostic, WrapErr};

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
        .register(&DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&NamedRunFilter::all_etiquettes().with_crate("app"))
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 0);

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
        .register(&DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&NamedRunFilter::all_etiquettes().with_crate("app"))
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
        .register(&DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&NamedRunFilter::all_etiquettes().with_crate("app"))
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
        .register(&DEPENDENCY_FRESHNESS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&NamedRunFilter::all_etiquettes().with_crate("app"))
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

fn dependency_freshness_survey_csv(store_root: &Path) -> miette::Result<String> {
    fs::read_to_string(
        store_root
            .join("findings")
            .join("dependency-freshness-survey.csv"),
    )
    .into_diagnostic()
    .wrap_err("dependency freshness survey csv")
}

fn write_dependency_freshness_cache(store_root: &Path, cache: &str) -> miette::Result<()> {
    let cache_dir = store_root.join("cache");
    fs::create_dir_all(&cache_dir)
        .into_diagnostic()
        .wrap_err("cache dir")?;
    fs::write(cache_dir.join("dependency-freshness.toml"), cache)
        .into_diagnostic()
        .wrap_err("dependency freshness cache")
}

fn write_workspace_manifest(root: &Path, manifest: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("workspace manifest")
}

fn write_member_manifest(root: &Path, rel: &str, manifest: &str) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(crate_root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n")
        .into_diagnostic()
        .wrap_err("lib.rs")
}

fn write_package_manifest(
    root: &Path,
    rel: &str,
    package_name: &str,
    version: &str,
) -> miette::Result<()> {
    write_member_manifest(
        root,
        rel,
        &format!(
            r#"
[package]
name = "{package_name}"
version = "{version}"
edition = "2024"
"#
        ),
    )
}

fn write_lockfile(root: &Path, lockfile: &str) -> miette::Result<()> {
    fs::write(root.join("Cargo.lock"), lockfile)
        .into_diagnostic()
        .wrap_err("lockfile")
}
