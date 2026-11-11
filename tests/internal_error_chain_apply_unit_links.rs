//! Conversion units across files: paths, macros, same-named declarations, distinct identities, and one-file visits.

use cordial::{ErrorConversionSiteKind, run_error_file_apply};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, source};

#[test]
fn conversion_unit_locates_cross_file_and_macro_paths() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "fixed".to_owned() } } }
macro_rules! hidden { () => { MessageError { message: "hidden".to_owned() } }; }
mod sibling;
"#;
    let fixture = fixture(original, "borrowed")?;
    fs::write(
        fixture.path().join("src/sibling.rs"),
        "use crate::MessageError;\nfn caller() { let _ = MessageError::new(); }\n",
    )
    .into_diagnostic()?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    assert!(
        unit.sites()
            .iter()
            .any(|site| site.kind() == ErrorConversionSiteKind::Macro && !site.resolved())
    );
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::Caller
            && site.path().ends_with("sibling.rs")
            && site
                .target()
                .as_ref()
                .is_some_and(|target| target.declaration() == &fixture.path().join("src/lib.rs"))
            && !site.resolved()
    }));
    assert!(!unit.is_closed());
    assert_eq!(summary.changed_files(), 0);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn conversion_unit_links_cross_file_error_edges_by_declaration() -> miette::Result<()> {
    cordial::init_tracing();
    let original = "#[derive(Debug)]\nstruct Kind;\nmod child;\n";
    let fixture = fixture(original, "borrowed")?;
    fs::write(
        fixture.path().join("src/child.rs"),
        r#"use crate::Kind as ImportedKind;
#[derive(Debug)]
struct MessageError { message: String, kind: ImportedKind }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_owned(), kind: ImportedKind } }
}
"#,
    )
    .into_diagnostic()?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.type_name() == "MessageError")
        .wrap_err("cross-file conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site.edge().as_ref().is_some_and(|edge| {
                edge.target().as_ref().is_some_and(|target| {
                    target.declaration() == &fixture.path().join("src/lib.rs")
                })
            })
            && !site.resolved()
    }));
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn conversion_unit_keeps_same_named_declarations_in_their_modules() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = fixture("mod a;\nmod b;\n", "borrowed")?;
    fs::create_dir_all(fixture.path().join("src/a")).into_diagnostic()?;
    fs::write(
        fixture.path().join("src/a/mod.rs"),
        "#[derive(Debug)]\nstruct Kind;\nmod child;\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/b.rs"),
        "#[derive(Debug)]\nstruct Kind;\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/a/child.rs"),
        r#"#[derive(Debug)]
struct MessageError { message: String, kind: super::Kind }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_owned(), kind: super::Kind } }
}
"#,
    )
    .into_diagnostic()?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.type_name() == "MessageError")
        .wrap_err("nested conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site.edge().as_ref().is_some_and(|edge| {
                edge.target().as_ref().is_some_and(|target| {
                    target.declaration() == &fixture.path().join("src/a/mod.rs")
                })
            })
    }));
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn conversion_targets_on_one_line_have_distinct_identities() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = fixture(
        r#"#[derive(Debug)] struct Kind; #[derive(Debug)] struct Other;
#[derive(Debug)] struct First { message: String, kind: Kind }
#[derive(Debug)] struct Second { message: String, kind: Other }
impl std::fmt::Display for First {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::fmt::Display for Second {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for First {}
impl std::error::Error for Second {}
impl First { fn new() -> Self { Self { message: "first".to_owned(), kind: Kind } } }
impl Second { fn new() -> Self { Self { message: "second".to_owned(), kind: Other } } }
"#,
        "borrowed",
    )?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let targets: Vec<_> = summary
        .conversion_units()
        .iter()
        .filter(|unit| matches!(unit.type_name().as_str(), "First" | "Second"))
        .flat_map(|unit| unit.sites())
        .filter(|site| site.kind() == ErrorConversionSiteKind::ErrorEdge)
        .filter_map(|site| site.edge().as_ref()?.target().as_ref())
        .collect();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].line(), targets[1].line());
    assert_ne!(targets[0].column(), targets[1].column());
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn custom_module_path_does_not_create_a_speculative_cross_file_link() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = fixture(
        "#[derive(Debug)]\nstruct Kind;\n#[path = \"dormant.rs\"] mod other;\n",
        "borrowed",
    )?;
    fs::write(
        fixture.path().join("src/dormant.rs"),
        r#"#[derive(Debug)]
struct MessageError { message: String, kind: crate::Kind }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_owned(), kind: crate::Kind } }
}
"#,
    )
    .into_diagnostic()?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.type_name() == "MessageError")
        .wrap_err("custom-path conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site
                .edge()
                .as_ref()
                .is_some_and(|edge| edge.target().is_none())
    }));
    Ok(())
}

#[test]
fn one_file_visit_keeps_sites_separate_for_multiple_error_fields() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "message".to_owned() } } }

#[derive(Debug)]
struct DetailError { detail: String }
impl std::fmt::Display for DetailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.detail) }
}
impl std::error::Error for DetailError {}
impl DetailError { fn new() -> Self { Self { detail: "detail".to_owned() } } }

fn callers() { let _ = MessageError::new(); let _ = DetailError::new(); }
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(summary.conversion_units().len(), 2);
    for (type_name, field_name) in [("MessageError", "message"), ("DetailError", "detail")] {
        let unit = summary
            .conversion_units()
            .iter()
            .find(|unit| unit.type_name() == type_name && unit.field_name() == field_name)
            .wrap_err("candidate conversion unit")?;
        assert_eq!(
            unit.sites()
                .iter()
                .filter(|site| site.kind() == ErrorConversionSiteKind::Constructor)
                .count(),
            1
        );
        assert_eq!(
            unit.sites()
                .iter()
                .filter(|site| site.kind() == ErrorConversionSiteKind::Caller)
                .count(),
            1
        );
    }
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn generated_default_constructor_is_inventory_blocker() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug, Default)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "fixed".to_owned() } } }
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    assert!(unit.sites().iter().any(|site| site.kind()
        == ErrorConversionSiteKind::GeneratedConstructor
        && !site.resolved()));
    assert_eq!(summary.changed_files(), 0);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}
