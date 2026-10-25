use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;

#[test]
fn private_error_graph_across_files_round_trips() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    check(root.path())?;

    let preview = run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.changed_files(), 2, "{:?}", preview.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(
        fs::read_to_string(&child).into_diagnostic()?,
        original_child
    );

    let borrowed = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 2, "{:?}", borrowed.blockers());
    let borrowed_library = fs::read_to_string(&library).into_diagnostic()?;
    let borrowed_child = fs::read_to_string(&child).into_diagnostic()?;
    assert!(borrowed_library.contains("source: &'a Cause"));
    assert!(borrowed_child.contains("source: source,"));
    check(root.path())?;

    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"owned\"\n",
    )
    .into_diagnostic()?;
    let owned = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 2, "{:?}", owned.blockers());
    check(root.path())?;
    Ok(())
}

#[test]
fn unresolved_external_field_read_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nfn read(error: &super::Parent<'_>) -> &super::Cause {{ &error.source }}\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn hidden_external_graph_name_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(
        &child,
        format!("{original_child}\nmacro_rules! hidden {{ () => {{ super::Parent }}; }}\n"),
    )
    .into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn same_named_external_declaration_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nstruct Parent;\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(fs::read_to_string(&library).into_diagnostic()?, original_library);
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-cross-file-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"mod child;
#[derive(Debug, Clone)]
struct Cause;
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cause") }
}
impl std::error::Error for Cause {}

#[derive(Debug, Clone)]
struct Parent<'a> { source: Cause, file: String, anchor: &'a () }
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_source() {
        use std::error::Error;
        let cause = super::Cause;
        let anchor = ();
        let error = super::child::make(&cause, &anchor);
        assert_eq!(error.source().unwrap().to_string(), "cause");
    }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/child.rs"),
        r#"use std::panic::Location;
#[track_caller]
pub(super) fn make<'a>(source: &'a super::Cause, anchor: &'a ()) -> super::Parent<'a> {
    super::Parent { source: source.clone(), file: Location::caller().file().to_string(), anchor }
}
"#,
    )
    .into_diagnostic()?;
    Ok(root)
}

fn check(root: &Path) -> miette::Result<()> {
    let output = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    if !output.status.success() {
        return Err(miette::miette!(
            "fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
