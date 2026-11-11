//! Fixture crates for the cross-file apply tests: a private error graph, and the same graph
//! distributed over `kind` and `build` modules.

use miette::IntoDiagnostic;
use std::fs;

pub fn distributed_fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-distributed-error-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"mod cause;
mod kind;
mod build;

#[derive(Debug, Clone)]
struct Parent<'a> {
    source: cause::Cause,
    kind: Box<kind::Kind<'a>>,
    file: String,
    anchor: &'a (),
}
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/cause.rs"),
        r#"use std::panic::Location;
#[derive(Debug, Clone)]
pub(super) struct Cause { file: String }
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Cause {}
impl Cause {
    #[track_caller]
    pub(super) fn new() -> Self { Self { file: Location::caller().file().to_string() } }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/kind.rs"),
        r#"#[derive(Debug, Clone)]
pub(super) enum Kind<'a> {
    Wrapped(Box<super::cause::Cause>),
    Marker(&'a ()),
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/build.rs"),
        r#"use std::panic::Location;

pub(super) fn kind<'a>(source: &'a super::cause::Cause, anchor: &'a ()) -> super::kind::Kind<'a> {
    let _ = anchor;
    super::kind::Kind::Wrapped(Box::new(source.clone()))
}

#[track_caller]
pub(super) fn parent<'a>(
    source: &'a super::cause::Cause,
    kind: &'a super::kind::Kind<'a>,
    anchor: &'a (),
) -> super::Parent<'a> {
    super::Parent {
        source: source.clone(),
        kind: Box::new(kind.clone()),
        file: Location::caller().file().to_string(),
        anchor,
    }
}
"#,
    )
    .into_diagnostic()?;
    Ok(root)
}

pub fn fixture() -> miette::Result<tempfile::TempDir> {
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
