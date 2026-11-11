//! Kind fan-out: literal call sites that feed one kind variant, counted across files and wrappers.

use miette::IntoDiagnostic;
use std::fs;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::{
    INTERNAL_ERROR_CHAIN_ETIQUETTE, InternalErrorComplianceId, RunAll, Session, SessionBuilder,
};

#[path = "support/error_chain_write_crate.rs"]
mod error_chain_write_crate;

use error_chain_write_crate::write_error_crate;

#[test]
fn kind_fanout_combines_local_wrappers_and_direct_sites_across_files() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src = fixture.path().join("src");
    fs::create_dir_all(&src).into_diagnostic()?;
    fs::write(src.join("lib.rs"), "mod error; mod alpha; mod beta;\n").into_diagnostic()?;
    fs::write(
        src.join("error.rs"),
        r#"
pub struct DomainError { kind: Box<DomainKind> }
pub enum DomainKind { Invalid(InvalidSource) }
pub struct InvalidSource { detail: String, file: String, line: u32 }
impl InvalidSource {
    pub fn new(detail: impl Into<String>) -> Self {
        Self { detail: detail.into(), file: String::new(), line: 0 }
    }
}
impl std::error::Error for InvalidSource {}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        src.join("alpha.rs"),
        r#"
use crate::error::{DomainError, DomainKind, InvalidSource};
fn invalid(detail: String) -> DomainError {
    DomainError::new(DomainKind::Invalid(InvalidSource::new(detail)))
}
fn a() { invalid("hour overflow".to_owned()); }
fn b() { invalid("month overflow".into()); }
"#,
    )
    .into_diagnostic()?;
    fs::write(
        src.join("beta.rs"),
        r#"
use crate::error::{DomainError, DomainKind, InvalidSource};
fn invalid(detail: String) -> DomainError {
    DomainError::new(DomainKind::Invalid(InvalidSource::new(detail)))
}
fn c() { invalid("day overflow".to_string()); }
fn d() { invalid("ambiguous local time".to_owned()); }
fn e() { DomainKind::Invalid(InvalidSource::new("bad offset")); }
fn f() { let _ = InvalidSource::new("invalid calendar"); }
"#,
    )
    .into_diagnostic()?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let fanout: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::ArchKindFanout001)
        .collect();
    assert_eq!(fanout.len(), 1, "{fanout:?}");
    assert_eq!(fanout[0].context(), "DomainKind::Invalid");
    assert_eq!(fanout[0].file(), &src.join("error.rs"));
    assert!(fanout[0].snippet().contains("6 literal call sites"));
    for literal in [
        "hour overflow",
        "month overflow",
        "day overflow",
        "ambiguous local time",
        "bad offset",
        "invalid calendar",
    ] {
        assert!(
            fanout[0].snippet().contains(literal),
            "missing {literal}: {}",
            fanout[0].snippet()
        );
    }
    Ok(())
}

/// `AmenableError::io`/`::invariant`-shaped wrappers: defined once as a
/// method inside `impl DomainError { }` (never a bare top-level `fn`),
/// then called through the qualified path `DomainError::invalid(...)`
/// from other files, some with `format!()` interpolation. Each of
/// those three traits independently hid the whole class of call from
/// fan-out detection before the fix in catalog/visitor.rs and
/// catalog/fanout.rs: `visit_item_fn` never saw the impl method,
/// `call_name` rejected the 2-segment call path, and even a
/// same-named free-function wrapper would have been keyed by its own
/// definition file, which the real call sites never share.
#[test]
fn kind_fanout_catches_impl_block_wrapper_called_by_qualified_path_across_files()
-> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src = fixture.path().join("src");
    fs::create_dir_all(&src).into_diagnostic()?;
    fs::write(src.join("lib.rs"), "mod error; mod alpha; mod beta;\n").into_diagnostic()?;
    fs::write(
        src.join("error.rs"),
        r#"
pub struct DomainError { kind: Box<DomainKind> }
pub enum DomainKind { Invalid(InvalidSource) }
pub struct InvalidSource { detail: String, file: String, line: u32 }
impl InvalidSource {
    pub fn new(detail: impl Into<String>) -> Self {
        Self { detail: detail.into(), file: String::new(), line: 0 }
    }
}
impl std::error::Error for InvalidSource {}
impl DomainError {
    pub fn new(kind: DomainKind) -> Self {
        Self { kind: Box::new(kind) }
    }
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new(DomainKind::Invalid(InvalidSource::new(detail)))
    }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        src.join("alpha.rs"),
        r#"
use crate::error::DomainError;
fn a(id: &str) { DomainError::invalid(format!("unknown id: {id}")); }
fn b() { DomainError::invalid("hour overflow"); }
"#,
    )
    .into_diagnostic()?;
    fs::write(
        src.join("beta.rs"),
        r#"
use crate::error::DomainError;
fn c(path: &str) { DomainError::invalid(format!("missing path: {path}")); }
fn d() { DomainError::invalid("month overflow"); }
"#,
    )
    .into_diagnostic()?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let fanout: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::ArchKindFanout001)
        .collect();
    assert_eq!(fanout.len(), 1, "{fanout:?}");
    assert_eq!(fanout[0].context(), "DomainKind::Invalid");
    for literal in [
        "unknown id: {id}",
        "hour overflow",
        "missing path: {path}",
        "month overflow",
    ] {
        assert!(
            fanout[0].snippet().contains(literal),
            "missing {literal}: {}",
            fanout[0].snippet()
        );
    }
    Ok(())
}

#[test]
fn kind_fanout_anchors_external_kind_at_wrapper_and_ignores_branching() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(
        r#"
fn invalid(detail: String) -> TemporalError {
    TemporalError::new(TemporalErrorKind::InvalidDescriptor(
        InvalidDescriptorSource::new(detail),
    ))
}
fn a() { invalid("hour".to_owned()); }
fn b() { invalid("month".to_owned()); }
fn c() { invalid("day".to_owned()); }
fn d() { invalid("offset".to_owned()); }
fn conditional(detail: String) -> TemporalError {
    if detail.is_empty() {
        TemporalError::new(TemporalErrorKind::InvalidDescriptor(InvalidDescriptorSource::new(detail)))
    } else {
        TemporalError::new(TemporalErrorKind::Unsupported(UnsupportedSource::new(detail)))
    }
}
fn e() { conditional("one".to_owned()); }
fn f() { conditional("two".to_owned()); }
fn g() { conditional("three".to_owned()); }
fn h() { conditional("four".to_owned()); }
"#,
    )?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let fanout: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::ArchKindFanout001)
        .collect();
    assert_eq!(fanout.len(), 1, "{fanout:?}");
    assert_eq!(fanout[0].context(), "TemporalErrorKind::InvalidDescriptor");
    assert_eq!(fanout[0].line(), 2);
    assert!(!fanout[0].snippet().contains("conditional"));
    Ok(())
}

#[test]
fn kind_fanout_reports_direct_external_source_calls() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(
        r#"
fn a() { let _ = InvalidDescriptorSource::new("hour"); }
fn b() { let _ = InvalidDescriptorSource::new("month"); }
fn c() { let _ = InvalidDescriptorSource::new("day"); }
fn d() { let _ = InvalidDescriptorSource::new("offset"); }
"#,
    )?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let fanout: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::ArchKindFanout001)
        .collect();
    assert_eq!(fanout.len(), 1, "{fanout:?}");
    assert_eq!(fanout[0].context(), "InvalidDescriptorSource::new");
    assert!(fanout[0].snippet().contains("4 literal call sites"));
    Ok(())
}

#[test]
fn kind_fanout_zero_threshold_from_cordial_toml_flags_one_site() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(
        r#"
fn a() { let _ = InvalidDescriptorSource::new("hour"); }
"#,
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[internal_error_chain]\nkind_fanout_max_call_sites = 0\n",
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*INTERNAL_ERROR_CHAIN_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic()?;
    assert!(outcome.findings().any(|finding| {
        finding.rule().id() == InternalErrorComplianceId::ArchKindFanout001.as_str()
    }));
    Ok(())
}

#[test]
fn kind_fanout_default_zero_flags_repeated_literal_sites() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_error_crate(
        r#"
fn invalid(detail: String) -> TemporalError {
    TemporalError::new(TemporalErrorKind::InvalidDescriptor(InvalidDescriptorSource::new(detail)))
}
fn a() { invalid("same".to_owned()); }
fn b() { invalid("same".to_owned()); }
fn c() { invalid("same".to_owned()); }
fn d() { invalid("same".to_owned()); }
"#,
    )?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(
        report
            .compliance()
            .findings()
            .iter()
            .any(|finding| { finding.rule_id() == InternalErrorComplianceId::ArchKindFanout001 })
    );
    Ok(())
}
