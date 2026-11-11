//! A private error graph rewrites as one unit and round-trips, and an unresolved source leaves every field unchanged.

use cordial::{ErrorFieldApplyBlockerReason, run_error_file_apply};
use miette::IntoDiagnostic;
use std::path::Path;
use std::process::Command;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, set_strategy, source};

fn test_fixture(root: &Path) -> miette::Result<()> {
    let result = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
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
fn private_error_graph_round_trips_as_one_unit() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
#[derive(Debug, Clone)]
struct Cause;
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cause") }
}
impl std::error::Error for Cause {}

#[derive(Debug, Clone)]
struct Leaf<'a> { source: Cause, file: String, anchor: &'a () }
impl<'a> Leaf<'a> {
    #[track_caller]
    fn new(source: &'a Cause, anchor: &'a ()) -> Self {
        Self { source: source.clone(), file: Location::caller().file().to_string(), anchor }
    }
}
impl std::fmt::Display for Leaf<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Leaf<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}

#[derive(Debug, Clone)]
enum Kind<'a> { Leaf(Leaf<'a>) }
fn make_kind<'a>(leaf: &'a Leaf<'a>) -> Kind<'a> { Kind::Leaf(leaf.clone()) }

#[derive(Debug)]
struct Parent<'a> { kind: Box<Kind<'a>>, source: Cause, file: String, anchor: &'a () }
impl<'a> Parent<'a> {
    #[track_caller]
    fn new(kind: &'a Kind<'a>, source: &'a Cause, anchor: &'a ()) -> Self {
        Self { kind: Box::new(kind.clone()), source: source.clone(), file: Location::caller().file().to_string(), anchor }
    }
    fn kind_label(&self) -> &'static str {
        match self.kind.as_ref() { Kind::Leaf(_) => "leaf" }
    }
}
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
impl<'a> From<(&'a Kind<'a>, &'a Cause, &'a ())> for Parent<'a> {
    fn from((kind, source, anchor): (&'a Kind<'a>, &'a Cause, &'a ())) -> Self {
        Self::new(kind, source, anchor)
    }
}
fn via_try<'a>(kind: &'a Kind<'a>, source: &'a Cause, anchor: &'a ()) -> Result<(), Parent<'a>> {
    let input: Result<(), (&Kind<'a>, &Cause, &())> = Err((kind, source, anchor));
    input?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn keeps_source_and_location() {
        let cause = Cause;
        let anchor = ();
        let leaf = Leaf::new(&cause, &anchor);
        let kind = make_kind(&leaf);
        let error = via_try(&kind, &cause, &anchor).unwrap_err();
        assert_eq!(error.source().unwrap().to_string(), "cause");
        assert_eq!(error.kind_label(), "leaf");
        assert!(error.to_string().contains("lib.rs"));
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    test_fixture(fixture.path())?;
    let dry_run =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(dry_run.changed_files(), 1);
    assert_eq!(source(fixture.path())?, original);
    let borrowed =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 1);
    let borrowed_source = source(fixture.path())?;
    assert!(borrowed_source.contains("kind: &'a Kind<'a>"));
    assert!(borrowed_source.contains("Leaf(&'a Leaf<'a>)"));
    assert!(borrowed_source.contains("source: &'a Cause"));
    test_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );
    set_strategy(fixture.path(), "owned")?;
    let owned =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 1);
    test_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );
    Ok(())
}

#[test]
fn lifetime_free_private_error_graph_round_trips_as_one_unit() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
#[derive(Debug, Clone)]
struct Cause;
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cause") }
}
impl std::error::Error for Cause {}

#[derive(Debug, Clone)]
struct Leaf { source: Cause, file: String }
impl Leaf {
    #[track_caller]
    fn new<'a>(source: &'a Cause) -> Self {
        Self { source: source.clone(), file: Location::caller().file().to_string() }
    }
}
impl std::fmt::Display for Leaf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Leaf {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}

#[derive(Debug, Clone)]
enum Kind { Leaf(Leaf) }
fn make_kind<'a>(leaf: &'a Leaf) -> Kind { Kind::Leaf(leaf.clone()) }

#[derive(Debug)]
struct Parent { kind: Box<Kind>, source: Cause, file: String }
impl Parent {
    #[track_caller]
    fn new<'a>(kind: &'a Kind, source: &'a Cause) -> Self {
        Self { kind: Box::new(kind.clone()), source: source.clone(), file: Location::caller().file().to_string() }
    }
    fn kind_label(&self) -> &'static str {
        match self.kind.as_ref() { Kind::Leaf(_) => "leaf" }
    }
}
impl std::fmt::Display for Parent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
impl<'a> From<(&'a Kind, &'a Cause)> for Parent {
    fn from((kind, source): (&'a Kind, &'a Cause)) -> Self { Self::new(kind, source) }
}
fn via_try<'a>(kind: &'a Kind, source: &'a Cause) -> Result<(), Parent> {
    let input: Result<(), (&Kind, &Cause)> = Err((kind, source));
    input?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn keeps_source_and_location() {
        let cause = Cause;
        let leaf = Leaf::new(&cause);
        let kind = make_kind(&leaf);
        let error = via_try(&kind, &cause).unwrap_err();
        assert_eq!(error.source().unwrap().to_string(), "cause");
        assert_eq!(error.kind_label(), "leaf");
        assert!(error.to_string().contains("lib.rs"));
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    test_fixture(fixture.path())?;
    let dry_run =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(dry_run.changed_files(), 1, "{:?}", dry_run.blockers());
    assert_eq!(source(fixture.path())?, original);
    let borrowed =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 1, "{:?}", borrowed.blockers());
    let borrowed_source = source(fixture.path())?;
    assert!(borrowed_source.contains("struct Parent<'a>"));
    assert!(borrowed_source.contains("enum Kind<'a>"));
    assert!(borrowed_source.contains("struct Leaf<'a>"));
    assert!(borrowed_source.contains("kind: &'a Kind<'a>"));
    assert!(borrowed_source.contains("Leaf(&'a Leaf<'a>)"));
    test_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );
    set_strategy(fixture.path(), "owned")?;
    let owned =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 1, "{:?}", owned.blockers());
    test_fixture(fixture.path())?;
    let owned_source = source(fixture.path())?;
    assert!(owned_source.contains("struct Parent {"));
    assert!(owned_source.contains("enum Kind {"));
    assert!(owned_source.contains("struct Leaf {"));
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    let blocked_source = format!(
        "{original}\nfn detached() -> Parent {{\n    let cause = Cause;\n    let leaf = Leaf::new(&cause);\n    let kind = make_kind(&leaf);\n    Parent::new(&kind, &cause)\n}}\n"
    );
    let blocked = self::fixture(&blocked_source, "borrowed")?;
    check_fixture(blocked.path())?;
    let result =
        run_error_file_apply(blocked.path(), blocked.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0);
    assert!(
        result
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(blocked.path())?, blocked_source);

    let misleading_lifetime = format!(
        "{original}\nfn detached<'a>(_: &'a ()) -> Parent {{\n    let cause = Cause;\n    let leaf = Leaf::new(&cause);\n    let kind = make_kind(&leaf);\n    Parent::new(&kind, &cause)\n}}\n"
    );
    let blocked = self::fixture(&misleading_lifetime, "borrowed")?;
    check_fixture(blocked.path())?;
    let result =
        run_error_file_apply(blocked.path(), blocked.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0);
    assert!(
        result
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(blocked.path())?, misleading_lifetime);

    let unanchored_source = original.replacen(
        "fn new<'a>(kind: &'a Kind, source: &'a Cause) -> Self",
        "fn new<'a>(kind: &'a Kind, source: &Cause) -> Self",
        1,
    );
    let blocked = self::fixture(&unanchored_source, "borrowed")?;
    check_fixture(blocked.path())?;
    let result =
        run_error_file_apply(blocked.path(), blocked.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0);
    assert!(
        result
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(blocked.path())?, unanchored_source);

    let local_source = original.replacen(
        "Self { source: source.clone(), file: Location::caller().file().to_string() }",
        "let owned = source.clone(); Self { source: owned.clone(), file: Location::caller().file().to_string() }",
        1,
    );
    let blocked = self::fixture(&local_source, "borrowed")?;
    check_fixture(blocked.path())?;
    let result =
        run_error_file_apply(blocked.path(), blocked.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0);
    assert!(
        result
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(blocked.path())?, local_source);
    Ok(())
}

#[test]
fn unresolved_source_graph_leaves_every_field_unchanged() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug, Clone)]
struct Cause;
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cause") }
}
impl std::error::Error for Cause {}
#[derive(Debug)]
struct Parent<'a> { source: Cause, file: String, anchor: &'a () }
impl<'a> Parent<'a> {
    fn new(anchor: &'a ()) -> Self {
        Self { source: Cause, file: "lib.rs".to_owned(), anchor }
    }
}
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0);
    assert!(
        summary
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}
