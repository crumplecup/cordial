use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::run_error_file_apply;
use miette::{IntoDiagnostic, WrapErr};

fn fixture(source: &str, strategy: &str) -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"file-apply-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), source).into_diagnostic()?;
    set_strategy(root.path(), strategy)?;
    Ok(root)
}

fn set_strategy(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
    .wrap_err("write strategy")
}

fn source(root: &Path) -> miette::Result<String> {
    fs::read_to_string(root.join("src/lib.rs")).into_diagnostic()
}

fn check_fixture(root: &Path) -> miette::Result<()> {
    let result = Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

#[test]
fn call_site_file_round_trips_between_strategies() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
struct SiteError { file: String, line: u32 }
impl std::fmt::Debug for SiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}
impl std::fmt::Display for SiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}
impl std::error::Error for SiteError {}
impl SiteError {
    #[track_caller]
    fn new() -> Self {
        let loc = Location::caller();
        Self { file: loc.file().to_string(), line: loc.line() }
    }
    #[track_caller]
    fn direct() -> Self {
        Self { file: Location::caller().file().to_owned(), line: 1 }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let preview = run_error_file_apply(root, root, None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 1);
    assert_eq!(source(root)?, original);

    let applied = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(applied.converted_fields(), 1);
    assert_eq!(applied.unresolved(), 0);
    let borrowed = source(root)?;
    assert!(borrowed.contains("file: &'static str"), "{borrowed}");
    assert!(borrowed.contains("file: loc.file()"), "{borrowed}");
    assert!(
        borrowed.contains("file: Location::caller().file()"),
        "{borrowed}"
    );
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let reverse = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(reverse.converted_fields(), 1);
    assert_eq!(
        source(root)?,
        original.replace(".to_owned()", ".to_string()")
    );
    check_fixture(root)?;
    Ok(())
}

#[test]
fn unrecognized_initializer_skips_entire_error_type() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl std::fmt::Debug for SiteError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) } }
impl std::fmt::Display for SiteError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) } }
impl SiteError {
    fn new(file: String) -> Self { Self { file } }
    fn caller() -> Self { Self { file: Location::caller().file().to_string() } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn public_error_type_is_not_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
pub struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl SiteError { fn new() -> Self { Self { file: Location::caller().file().to_string() } } }
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn qualified_constructor_and_shadowed_location_are_not_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let qualified = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl SiteError { fn new() -> Self { crate::SiteError { file: Location::caller().file().to_string() } } }
"#;
    let fixture = fixture(qualified, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, qualified);

    let shadowed = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl SiteError {
    fn new() -> Self {
        let loc = Location::caller();
        {
            let loc = other_location();
            Self { file: loc.file().to_string() }
        }
    }
}
"#;
    fs::write(fixture.path().join("src/lib.rs"), shadowed).into_diagnostic()?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, shadowed);
    Ok(())
}
