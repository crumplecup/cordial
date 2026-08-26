use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};

/// A minimal real `Cargo.toml` -- needed by any test whose fixture
/// relies on workspace-wide crate discovery via `cargo_metadata` (e.g.
/// `CallGraphFacts`'s own crate walk), which a bare directory with no
/// manifest at all is invisible to.
fn write_minimal_crate_manifest(root: &Path, crate_name: &str) -> miette::Result<()> {
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("crate manifest")
}

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
fn tracing_non_result_error_site_is_not_silent() -> miette::Result<()> {
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
        !csv.contains("TRACING-ERROR-PATH-SILENT"),
        "Option/non-Result bodies are not silent-error: {csv}"
    );
    Ok(())
}

#[test]
fn tracing_option_try_is_not_silent() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn lookup_first() -> Option<u8> {
    let value = std::fs::read_to_string("x").ok()?;
    value.bytes().next()
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
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        !csv.contains("TRACING-ERROR-PATH-SILENT"),
        "Option ? is absence, not a silent error: {csv}"
    );
    assert!(
        !csv.contains("TRACING-ERR-MISSING"),
        "Option lookup recipe has no err: {csv}"
    );
    Ok(())
}

#[test]
fn tracing_reports_private_and_pub_super() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
mod inner {
    pub(super) fn hidden() {}
}

pub fn visible() {}

fn private_helper() {}
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
    assert!(csv.contains("private_helper"), "{csv}");
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

#[test]
fn tracing_const_fn_is_never_flagged() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub struct Registration;

impl Registration {
    pub const fn new(id: u32) -> Self {
        let _ = id;
        Self
    }
}

pub fn traced_ordinary_fn() {}
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
        1,
        "only traced_ordinary_fn should be flagged -- `#[instrument]` can never \
         be written on `const fn new`, so it must never be proposed"
    );

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("traced_ordinary_fn"), "{checklist}");
    assert!(
        !checklist.contains("Registration::new"),
        "const fn must never appear in the checklist: {checklist}"
    );
    Ok(())
}

#[test]
fn tracing_err_recipe_requires_confirmed_display() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotDisplayable {
    offending_path: String,
}

#[derive(Debug, Clone, derive_more::Display)]
#[display("displayable error: {message}")]
pub struct DisplayableError {
    message: String,
}

pub fn returns_non_displayable_err() -> Result<(), NotDisplayable> {
    Ok(())
}

pub fn returns_displayable_err() -> Result<(), DisplayableError> {
    Ok(())
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
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    let non_displayable: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("returns_non_displayable_err"))
        .collect();
    assert_eq!(non_displayable.len(), 1, "{non_displayable:?}");
    assert!(
        !non_displayable[0].contains("err("),
        "NotDisplayable has no Display impl -- err() would propose code \
         that can't compile: {}",
        non_displayable[0]
    );

    let displayable: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("returns_displayable_err"))
        .collect();
    assert_eq!(displayable.len(), 1, "{displayable:?}");
    assert!(
        displayable[0].contains("err("),
        "DisplayableError derives derive_more::Display -- err() is safe: {}",
        displayable[0]
    );
    Ok(())
}

#[test]
fn tracing_skip_covers_tuple_destructured_generic_params() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Check<T> {
    fn ensures(pair: (T, T)) -> bool;
}

pub struct Checker<T>(std::marker::PhantomData<T>);

impl<T: PartialEq> Check<T> for Checker<T> {
    fn ensures((actual, expected): (T, T)) -> bool {
        actual == expected
    }
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
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let csv = fs::read_to_string(store.path().join("findings").join("tracing-instrument.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    let ensures: Vec<_> = csv
        .lines()
        .filter(|line| line.contains("ensures"))
        .collect();
    assert_eq!(ensures.len(), 1, "{ensures:?}");
    assert!(
        ensures[0].contains("skip(actual, expected)"),
        "a generic `T` destructured out of a tuple parameter needs to be \
         skipped individually -- tracing::instrument records each binding \
         found in a pattern via Debug, not the pattern's own top-level \
         type, and bare `T` has no Debug bound here: {}",
        ensures[0]
    );
    Ok(())
}

#[test]
fn tracing_never_flags_function_nested_in_configured_gate_cfg() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[cfg(kani)]
mod proofs {
    pub fn proof_harness() {}
}

pub fn traced_ordinary_fn() {}
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
        1,
        "proof_harness only exists under #[cfg(kani)], the same cfg name \
         this crate's own apply_gate_crates entry names -- #[instrument] \
         can never fire there in any real build, so it must never become \
         an open checklist item at all, only traced_ordinary_fn should"
    );

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("traced_ordinary_fn"), "{checklist}");
    assert!(!checklist.contains("proof_harness"), "{checklist}");
    Ok(())
}

#[test]
fn tracing_never_flags_function_called_only_from_proof_context() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}

pub fn traced_ordinary_fn() {}
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
        1,
        "Checker::ensures's only known caller is proofs::harness, itself \
         nested in #[cfg(kani)] -- neither is ever recorded as needing \
         #[instrument], discovered via call-graph reachability rather \
         than a hardcoded trait name; only traced_ordinary_fn should be \
         open"
    );

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("traced_ordinary_fn"), "{checklist}");
    assert!(!checklist.contains("Checker"), "{checklist}");
    assert!(!checklist.contains("harness"), "{checklist}");
    Ok(())
}

#[test]
fn tracing_still_flags_function_with_an_ordinary_caller_too() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}

pub fn validate(value: u32) -> bool {
    Checker::ensures(value)
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
        .with_store_home(store.path())
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
        "Checker::ensures also has a real, ordinary caller (validate), \
         so it must stay open (Gated) even though it also happens to be \
         called from proofs::harness -- a function is only ever excluded \
         once ALL its known callers are proof-only, never merely SOME of \
         them"
    );
    Ok(())
}

#[test]
fn tracing_treats_cfg_attr_gated_instrument_as_present() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[cfg_attr(not(kani), tracing::instrument(level = "debug"))]
pub fn scan_tree() {}
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
        0,
        "apply writes #[cfg_attr(not(kani), tracing::instrument(..))] for \
         gated crates; a later quality run must treat that as present, not \
         as a missing-instrument (or recipe-delta) gap"
    );
    Ok(())
}

fn open_rule_ids(outcome: &dyn cordial::RunOutcome) -> Vec<String> {
    outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .map(|finding| finding.rule().id().to_string())
        .collect()
}

#[test]
fn tracing_proof_only_method_with_instrument_is_flagged() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[tracing::instrument(level = "trace")]
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}

pub fn traced_ordinary_fn() {}
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
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert!(
        rules.iter().any(|id| id == "TRACING-PROOF-INSTRUMENT"),
        "Ensures::ensures is only called from a kani harness — the span \
         must come off, not stay as a missing-instrument or gated recipe: {rules:?}"
    );
    assert!(
        rules.iter().any(|id| id == "TRACING-MISSING-INSTRUMENT"),
        "ordinary traced_ordinary_fn still wants a span: {rules:?}"
    );
    assert_eq!(rules.len(), 2, "{rules:?}");
    Ok(())
}

#[test]
fn tracing_proof_only_gated_instrument_never_fires_so_it_is_flagged() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[cfg_attr(not(kani), tracing::instrument(level = "trace"))]
    fn ensures(input: T) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
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
        .with_store_home(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-PROOF-INSTRUMENT"],
        "gating not(kani) on a proof-only Ensures impl is a dead span — \
         the method never runs outside the prover: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_ungated_instrument_on_ordinary_fn_in_gate_crate() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn scan_tree() {}
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
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-UNGATED-INSTRUMENT"],
        "bare #[instrument] on ordinary code in a kani crate is visible \
         to the prover; attenuation is gate, not remove: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_skip_crate_with_instrument_is_flagged() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn scan_tree() {}

pub fn other_fn() {}
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
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-SKIP-INSTRUMENT"],
        "skip-policy (Verus/Creusot) must not keep #[instrument]; \
         uninstrumented other_fn must not get a missing-instrument push: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_skip_crate_uninstrumented_is_silent() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "\npub fn scan_tree() {}\n",
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
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert!(
        rules.is_empty(),
        "skip-policy with no span: do not push toward adding one: {rules:?}"
    );
    Ok(())
}
