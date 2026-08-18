use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};

#[test]
fn tracing_etiquette_detects_missing_instrument() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
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
        .register(&TRACING_ETIQUETTE)
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
        2,
        "traced + untraced are pub without instrument"
    );

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("untraced"));
    assert!(csv.contains("traced"));

    let checklist = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open gaps:** 2"));
    assert!(checklist.contains("untraced"));
    assert!(checklist.contains("TRACING-MISSING-INSTRUMENT"));
    assert!(checklist.contains("### `other`"));

    let summary = fs::read_to_string(findings_dir.join("tracing-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** open gaps"));
    assert!(summary.contains("| Crate | Open |"));
    Ok(())
}

#[test]
fn tracing_etiquette_classifies_roles_and_recipes() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
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
}

pub fn scan_source_tree() {}

pub fn load_session_config() -> Result<(), String> {
    Ok(())
}

pub fn run() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&TRACING_ETIQUETTE)
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
    assert!(csv.contains(",src/lib.rs,"));
    assert!(!csv.contains("/src/lib.rs,"));

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
    assert!(checklist.contains("src/lib.rs:"));
    assert!(checklist.contains("level = \"trace\""));
    assert!(checklist.contains("level = \"debug\""));
    assert!(checklist.contains("level = \"info\""));
    assert!(checklist.contains("err(level = \"warn\")"));
    Ok(())
}

#[test]
fn attribute_nodes_materialized_for_instrument() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
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
        .register(&TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let slug = cordial::project_slug_from_path(fixture.path());
    let cache = fs::read_to_string(store.path().join("cache").join(format!("{slug}.ir.json")))
        .into_diagnostic()
        .wrap_err("ir cache")?;
    assert!(cache.contains("Attribute"));
    assert!(cache.contains("HasAttr"));
    assert!(cache.contains("tracing::instrument"));
    Ok(())
}

#[test]
fn tracing_etiquette_emits_recipe_deltas() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
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
        .register(&TRACING_ETIQUETTE)
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

#[cfg(feature = "error_sites")]
#[test]
fn tracing_joins_error_sites_for_silent_path() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn scan_source_tree() {
    let _ = std::fs::read("missing").map_err(|_| ());
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
        .register(&TRACING_ETIQUETTE)
        .register(&cordial::ERROR_SITES_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("TRACING-ERROR-PATH-SILENT"),
        "error-site join should mark a non-Result scan as silent: {csv}"
    );
    assert!(csv.contains("scan_source_tree"));
    Ok(())
}

#[test]
fn tracing_include_pub_super_from_config() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\ninclude_pub_super = true\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
mod inner {
    pub(super) fn hidden() {}
}

pub fn visible() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("hidden"), "{csv}");
    assert!(csv.contains("visible"), "{csv}");
    Ok(())
}

#[test]
fn tracing_extra_skip_from_config() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\nextra_skip = [\"payload\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn scan_items(payload: Vec<u8>) {}\n",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(
        checklist.contains("skip(payload)"),
        "extra_skip should land in the recipe: {checklist}"
    );
    Ok(())
}
