//! Conversion units: constructors, callers and error edges, public and unsupported edges, and borrowed lifetimes.

use cordial::{
    ErrorConversionSiteKind, ErrorEdgeField, ErrorEdgeStorage, ErrorEdgeTargetKind,
    ErrorFieldApplyBlockerReason, ErrorTryScope, run_error_file_apply,
};
use miette::{IntoDiagnostic, WrapErr};

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, source};

#[test]
fn conversion_unit_records_constructors_callers_and_error_edges() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
enum Kind { Basic }
#[derive(Debug)]
struct MessageError { message: String, source: std::io::Error, kind: Box<Kind> }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for MessageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
impl MessageError {
    fn new(source: std::io::Error) -> Self {
        Self { message: "fixed".to_owned(), source, kind: Box::new(Kind::Basic) }
    }
}
impl From<std::io::Error> for MessageError {
    fn from(source: std::io::Error) -> Self { Self::new(source) }
}
fn caller() -> Result<(), MessageError> {
    std::fs::read("missing")?;
    let explicit = || -> Result<(), std::io::Error> {
        std::fs::read("missing")?;
        Ok(())
    };
    let inferred = || {
        std::fs::read("missing")?;
        Ok::<(), MessageError>(())
    };
    let pending = async {
        std::fs::read("missing")?;
        Ok::<(), MessageError>(())
    };
    let _ = (explicit, inferred, pending);
    let _ = MessageError::new(std::io::Error::other("x"));
    Ok(())
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    let kinds: Vec<_> = unit.sites().iter().map(|site| site.kind()).collect();
    for kind in [
        ErrorConversionSiteKind::Constructor,
        ErrorConversionSiteKind::Caller,
        ErrorConversionSiteKind::TypeUse,
        ErrorConversionSiteKind::FromConversion,
        ErrorConversionSiteKind::SourceMethod,
        ErrorConversionSiteKind::TryConversion,
        ErrorConversionSiteKind::ErrorEdge,
    ] {
        assert!(kinds.contains(&kind), "missing {kind:?}: {kinds:?}");
    }
    assert!(!unit.is_closed(), "the `?` conversion is not resolved");
    assert!(unit.sites().iter().all(|site| site.line() > 0));
    let try_sites: Vec<_> = unit
        .sites()
        .iter()
        .filter(|site| site.kind() == ErrorConversionSiteKind::TryConversion)
        .collect();
    assert_eq!(
        try_sites.len(),
        3,
        "explicit closure returns a different error"
    );
    assert!(try_sites.iter().any(|site| {
        !site.resolved()
            && site.try_conversion().as_ref().is_some_and(|evidence| {
                evidence.scope() == ErrorTryScope::Function
                    && evidence
                        .return_type_path()
                        .as_ref()
                        .is_some_and(|path| path == &["Result"])
                    && evidence
                        .error_type_path()
                        .as_ref()
                        .is_some_and(|path| path == &["MessageError"])
            })
    }));
    for scope in [ErrorTryScope::InferredClosure, ErrorTryScope::AsyncBlock] {
        assert!(try_sites.iter().any(|site| {
            site.try_conversion().as_ref().is_some_and(|evidence| {
                evidence.scope() == scope
                    && evidence.return_type_path().is_none()
                    && evidence.error_type_path().is_none()
            })
        }));
    }
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::FromConversion
            && !site.resolved()
            && site.from_conversion().as_ref().is_some_and(|evidence| {
                evidence.trait_path() == &["From"]
                    && evidence.input_type_path().as_ref().is_some_and(|path| {
                        path.iter().map(String::as_str).eq(["std", "io", "Error"])
                    })
                    && evidence
                        .output_type_path()
                        .as_ref()
                        .is_some_and(|path| path.iter().map(String::as_str).eq(["MessageError"]))
            })
    }));
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::SourceMethod
            && !site.resolved()
            && site.source_method().as_ref().is_some_and(|evidence| {
                evidence.trait_path() == &["std", "error", "Error"]
                    && evidence.return_type_text().as_deref()
                        == Some("Option<&(dyn std::error::Error + 'static)>")
                    && evidence.self_field_uses().contains("source")
            })
    }));
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site.related_type().as_deref() == Some("std::io::Error")
            && site.edge().as_ref().is_some_and(|edge| {
                edge.field() == ErrorEdgeField::Source
                    && edge.storage() == &ErrorEdgeStorage::Owned
                    && edge.type_path() == &["std", "io", "Error"]
            })
    }));
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && !site.resolved()
            && site.edge().as_ref().is_some_and(|edge| {
                edge.field() == ErrorEdgeField::Kind
                    && edge.storage() == &ErrorEdgeStorage::Boxed
                    && edge.type_path() == &["Kind"]
                    && edge.target().as_ref().is_some_and(|target| {
                        target.kind() == ErrorEdgeTargetKind::Enum
                            && target.declaration().ends_with("src/lib.rs")
                            && target.source_digest() == unit.source_digest()
                    })
            })
    }));
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    let blocked =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(blocked.changed_files(), 0);
    assert!(
        blocked
            .blockers()
            .iter()
            .any(|blocker| blocker.reason() == ErrorFieldApplyBlockerReason::GraphClosure)
    );
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn public_error_edge_target_does_not_close_the_unit() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
pub struct Kind;
#[derive(Debug)]
struct MessageError { message: String, kind: Kind }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_owned(), kind: Kind } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .first()
        .wrap_err("conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site
                .edge()
                .as_ref()
                .is_some_and(|edge| edge.type_path() == &["Kind"] && edge.target().is_none())
            && !site.resolved()
    }));
    assert!(!unit.is_closed());
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn unsupported_error_edge_shape_stays_unresolved() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String, source: Option<std::io::Error> }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_owned(), source: None } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .first()
        .wrap_err("conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && !site.resolved()
            && site.edge().is_none()
    }));
    assert!(!unit.is_closed());
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn borrowed_error_edge_carries_named_lifetime() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct Kind;
#[derive(Debug)]
struct MessageError<'a> { message: String, kind: &'a Kind }
impl std::fmt::Display for MessageError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError<'_> {}
impl<'a> MessageError<'a> {
    fn new(kind: &'a Kind) -> Self { Self { message: "fixed".to_owned(), kind } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .first()
        .wrap_err("conversion unit")?;
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site.edge().as_ref().is_some_and(|edge| {
                edge.field() == ErrorEdgeField::Kind
                    && edge.storage()
                        == &ErrorEdgeStorage::Borrowed {
                            lifetime: "a".to_string(),
                        }
                    && edge.type_path() == &["Kind"]
            })
    }));
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}
