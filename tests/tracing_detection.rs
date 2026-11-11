//! What the tracing lint reports: missing `#[instrument]`, roles and recipes, and the deltas it emits.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_manifest.rs"]
mod tracing_manifest;

use tracing_manifest::write_minimal_crate_manifest;

#[test]
fn tracing_keeps_cfg_alternative_declarations_separate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    write_minimal_crate_manifest(fixture.path(), "cfg_alternatives")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"#[cfg(feature = "alternate")]
#[tracing::instrument(level = "trace", skip_all)]
pub fn choose() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(not(feature = "alternate"))]
#[tracing::instrument(level = "trace", skip_all, err(level = "warn"))]
pub fn choose() -> Result<(), std::io::Error> { Ok(()) }
"#,
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;
    let csv = fs::read_to_string(store.path().join("findings/tracing-instrument.csv"))
        .into_diagnostic()?;
    let rows: Vec<_> = csv
        .lines()
        .filter(|line| line.contains(",choose,"))
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "cfg alternatives must retain both declarations: {csv}"
    );
    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings/tracing-instrument.checklist.md"),
    )
    .into_diagnostic()?;
    assert!(checklist.contains("src/lib.rs:1"), "{checklist}");
    assert!(
        !checklist.contains("src/lib.rs:5"),
        "a sibling's missing err must not contaminate the compliant declaration: {checklist}"
    );
    assert!(checklist.contains("TRACING-ERR-MISSING"), "{checklist}");
    Ok(())
}

#[test]
fn tracing_etiquette_detects_missing_instrument() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn instrumented() {
    let _ = 0;
}

pub fn traced() {
    let _ = 1;
}

pub fn untraced() {
    let _ = 2;
}

fn private_fn() {
    let _ = 3;
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        3,
        "traced + untraced + private_fn all lack instrument"
    );

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("untraced"));
    assert!(csv.contains("traced"));
    assert!(csv.contains("private_fn"));

    let checklist = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open gaps:** 3"));
    assert!(checklist.contains("untraced"));
    assert!(checklist.contains("private_fn"));
    assert!(checklist.contains("TRACING-MISSING-INSTRUMENT"));
    assert!(checklist.contains("### `other`"));

    let summary = fs::read_to_string(findings_dir.join("tracing-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**3** open gaps"));
    assert!(summary.contains("| Crate | Open |"));
    Ok(())
}

#[test]
fn tracing_etiquette_classifies_roles_and_recipes() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub struct Store {
    root: String,
}

impl Store {
    pub fn new(root: String) -> Self {
        Self { root }
    }

    pub fn as_str(&self) -> &str {
        &self.root
    }

    pub fn cache_dir(&self) -> String {
        format!("{}/cache", self.root)
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        Ok(self.root.clone())
    }
}

pub fn scan_source_tree() {}

pub fn load_session_config() -> Result<(), String> {
    Ok(())
}

pub fn build_report_summary() {}

pub fn run() {}

pub fn run_apply_patches() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("constructor"));
    assert!(csv.contains("getter"));
    assert!(csv.contains("scan"));
    assert!(csv.contains("io"));
    assert!(csv.contains("entry"));
    assert!(csv.contains("render"));
    assert!(csv.contains(",src/lib.rs,"));
    assert!(!csv.contains("/src/lib.rs,"));
    let pretty: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("to_json_pretty"))
        .collect();
    assert_eq!(pretty.len(), 1, "{pretty:?}");
    assert!(
        pretty[0].contains(",other,"),
        "Result to_* is encoding, not a getter: {}",
        pretty[0]
    );
    assert!(pretty[0].contains("err(level"), "{}", pretty[0]);
    let run_apply: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("run_apply_patches"))
        .collect();
    assert_eq!(run_apply.len(), 1, "{run_apply:?}");
    assert!(run_apply[0].contains(",entry,"), "{}", run_apply[0]);
    let summary: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("build_report_summary"))
        .collect();
    assert_eq!(summary.len(), 1, "{summary:?}");
    assert!(summary[0].contains(",render,"), "{}", summary[0]);

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("### `constructor`"));
    assert!(checklist.contains("### `getter`"));
    assert!(checklist.contains("### `entry`"));
    assert!(checklist.contains("### `render`"));
    assert!(checklist.contains("src/lib.rs:"));
    assert!(checklist.contains("level = \"trace\""));
    assert!(checklist.contains("level = \"debug\""));
    assert!(checklist.contains("level = \"info\""));
    assert!(checklist.contains("err(level = \"warn\")"));
    Ok(())
}

#[test]
fn attribute_nodes_materialized_for_instrument() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"#[tracing::instrument]
pub fn ok() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .with_ir_snapshot()
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let slug = "fixture";
    let ir =
        cordial::CrateIr::read_cache(&store.path().join("cache").join(format!("{slug}.ir.zst")))
            .into_diagnostic()
            .wrap_err("ir cache")?;
    let cache = serde_json::to_string(&ir.snapshot().into_diagnostic()?).into_diagnostic()?;
    assert!(cache.contains("Attribute"));
    assert!(cache.contains("HasAttr"));
    assert!(cache.contains("tracing::instrument"));
    Ok(())
}

#[test]
fn tracing_etiquette_emits_recipe_deltas() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[tracing::instrument(level = "debug", ret)]
    pub fn new(crate_name: String) -> Self {
        Self { root: crate_name }
    }

    #[tracing::instrument(level = "trace")]
    pub fn as_str(&self) -> &str {
        &self.root
    }

    #[tracing::instrument]
    pub fn cache_dir(&self) -> String {
        format!("{}/cache", self.root)
    }
}

#[tracing::instrument(level = "info")]
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}

#[tracing::instrument(level = "info")]
pub fn load_with_warn() -> Result<(), String> {
    tracing::warn!("failed");
    Ok(())
}

#[tracing::instrument(level = "info", err(level = "warn"))]
pub fn load_with_err() -> Result<(), String> {
    Ok(())
}

#[tracing::instrument(level = "info")]
pub fn run(crate_name: &str) {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("TRACING-FIELDS-MISSING"));
    assert!(csv.contains("TRACING-SKIP-MISSING"));
    assert!(csv.contains("TRACING-LEVEL-MISMATCH"));
    assert!(csv.contains("TRACING-ERR-MISSING"));
    assert!(csv.contains("TRACING-ERROR-PATH-SILENT"));
    assert!(csv.contains("Store::new"));
    assert!(csv.contains("Store::as_str"));
    assert!(csv.contains("Store::cache_dir"));
    assert!(csv.contains("load_session_config"));
    assert!(csv.contains("load_with_warn"));
    assert!(csv.contains("run"));
    assert!(
        !csv.contains("load_with_err"),
        "err(level) covers both err-missing and silent-path: {csv}"
    );
    assert!(
        !csv.contains("TRACING-MISSING-INSTRUMENT"),
        "all fixtures are already instrumented: {csv}"
    );

    let load_warn_rules: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("load_with_warn"))
        .collect();
    assert_eq!(load_warn_rules.len(), 1, "{load_warn_rules:?}");
    assert!(load_warn_rules[0].contains("TRACING-ERR-MISSING"));

    let load_cfg_rules: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("load_session_config"))
        .collect();
    assert_eq!(load_cfg_rules.len(), 2, "{load_cfg_rules:?}");
    Ok(())
}
