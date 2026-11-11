//! Rules on a single site: silent error paths, visibility, config skips, `const fn`, and the `err` recipe.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[cfg(feature = "error_sites")]
#[test]
fn tracing_non_result_error_site_is_not_silent() -> miette::Result<()> {
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
        .register(&*TRACING_ETIQUETTE)
        .register(&*cordial::ERROR_SITES_ETIQUETTE)
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
        .register(&*TRACING_ETIQUETTE)
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
        .register(&*TRACING_ETIQUETTE)
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
        .register(&*TRACING_ETIQUETTE)
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotDisplayable {
    offending_path: String,
}

#[derive(Debug, Clone)]
pub struct DisplayableError {
    message: String,
}

impl std::fmt::Display for DisplayableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "displayable error: {}", self.message)
    }
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
        .register(&*TRACING_ETIQUETTE)
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
        "DisplayableError implements Display -- err() is safe: {}",
        displayable[0]
    );
    Ok(())
}

#[test]
fn tracing_skip_covers_tuple_destructured_generic_params() -> miette::Result<()> {
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
        .register(&*TRACING_ETIQUETTE)
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
