use std::fs;
use std::process::Command;

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;

#[test]
fn one_crate_target_updates_two_source_files() -> miette::Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-write-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    let first = r#"mod child;
#[derive(Debug)]
struct FirstError { message: String }
impl std::fmt::Display for FirstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for FirstError {}
impl FirstError { fn new() -> Self { Self { message: "first".to_owned() } } }
"#;
    let second = r#"#[derive(Debug)]
struct SecondError { detail: String }
impl std::fmt::Display for SecondError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.detail) }
}
impl std::error::Error for SecondError {}
impl SecondError { fn new() -> Self { Self { detail: "second".to_owned() } } }
"#;
    let lib = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    fs::write(&lib, first).into_diagnostic()?;
    fs::write(&child, second).into_diagnostic()?;

    let preview = run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.changed_files(), 2);
    assert_eq!(fs::read_to_string(&lib).into_diagnostic()?, first);
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, second);

    let applied = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(applied.changed_files(), 2);
    assert!(
        fs::read_to_string(&lib)
            .into_diagnostic()?
            .contains("message: &'static str")
    );
    assert!(
        fs::read_to_string(&child)
            .into_diagnostic()?
            .contains("detail: &'static str")
    );
    let checked = Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(root.path())
        .output()
        .into_diagnostic()?;
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    Ok(())
}
