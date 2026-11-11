//! Compliance violations: stringified and discarded errors, preserved propagation, wrappers, and proc-macro crates.

use miette::IntoDiagnostic;
use std::fs;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::InternalErrorComplianceId;

#[path = "support/error_chain_crate.rs"]
mod error_chain_crate;

use error_chain_crate::crate_with_lib;

const COMPLIANCE_FIXTURE: &str = include_str!("fixtures/error_chain/compliance.rs");

/// Compliance findings for `source`; `proc_macro` marks the crate a proc-macro lib.
fn compliance_findings(
    source: &str,
    proc_macro: bool,
) -> miette::Result<Vec<cordial::InternalErrorComplianceFinding>> {
    let fixture = crate_with_lib(source, proc_macro)?;
    Ok(error_reports::internal_report(fixture.path(), "fixture")?
        .compliance()
        .findings()
        .clone())
}

#[test]
fn tracked_map_err_function_items_are_flagged_but_ordinary_converters_are_not() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src = fixture.path().join("src");
    fs::create_dir_all(&src).into_diagnostic()?;
    let source = r#"
struct TrackedError { file: &'static str, line: u32 }
impl std::fmt::Display for TrackedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "tracked") }
}
impl std::error::Error for TrackedError {}
impl From<std::io::Error> for TrackedError {
    #[track_caller]
    fn from(_: std::io::Error) -> Self {
        let caller = std::panic::Location::caller();
        Self { file: caller.file(), line: caller.line() }
    }
}
impl TrackedError {
    #[track_caller]
    fn wrap(_: std::io::Error) -> Self {
        let caller = std::panic::Location::caller();
        Self { file: caller.file(), line: caller.line() }
    }
    fn plain(_: std::io::Error) -> Self { Self { file: "plain", line: 0 } }
}
fn sites() {
    let _ = std::fs::read("x").map_err(TrackedError::from);
    let _ = std::fs::read("x").map_err(TrackedError::wrap);
    let _ = std::fs::read("x").map_err(TrackedError::plain);
    let _ = std::fs::read("x").map_err(|err| TrackedError::from(err));
}
"#;
    fs::write(src.join("lib.rs"), source).into_diagnostic()?;
    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    let findings: Vec<_> = report
        .compliance()
        .findings()
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::MapErrTrackCaller001)
        .collect();
    assert_eq!(findings.len(), 2);
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet().contains("TrackedError::from"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet().contains("TrackedError::wrap"))
    );
    Ok(())
}

#[test]
fn stringify_map_err_is_compliance_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = compliance_findings(COMPLIANCE_FIXTURE, false)?;
    assert!(findings.iter().any(|finding| {
        finding.rule_id() == InternalErrorComplianceId::StringifyForeign001
            && finding.context().contains("stringify_foreign")
    }));
    Ok(())
}

#[test]
fn discard_typed_error_is_compliance_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = compliance_findings(COMPLIANCE_FIXTURE, false)?;
    assert!(findings.iter().any(|finding| {
        finding.rule_id() == InternalErrorComplianceId::DiscardTyped001
            && finding.context().contains("discard_typed")
    }));
    assert!(findings.iter().any(|finding| {
        finding.rule_id() == InternalErrorComplianceId::DiscardTyped001
            && finding.context().contains("if_let_discard")
    }));
    Ok(())
}

#[test]
fn preserved_foreign_propagation_is_not_compliance_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = compliance_findings(COMPLIANCE_FIXTURE, false)?;
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context().contains("preserved"))
    );
    Ok(())
}

#[test]
fn syn_parse_and_from_wrappers_are_not_compliance_violations() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = compliance_findings(COMPLIANCE_FIXTURE, false)?;
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context().contains("wrap_syn")
                || finding.context().contains("wrap_from")
                || finding.context().contains("wrap_json")
                || finding.context().contains("wrap_cargo_metadata")),
        "typed wrappers that keep the foreign error must not be discards: {findings:?}"
    );
    Ok(())
}

#[test]
fn format_interpolation_of_error_binding_is_compliance_violation() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = compliance_findings(COMPLIANCE_FIXTURE, false)?;
    assert!(
        findings.iter().any(|finding| {
            finding.context().contains("stringify_via_format")
                && matches!(
                    finding.rule_id(),
                    InternalErrorComplianceId::DiscardTyped001
                        | InternalErrorComplianceId::StringifyForeign001
                )
        }),
        "invariant(format!(…{{err}})) must be a compliance hit: {findings:?}"
    );
    Ok(())
}

#[test]
fn stringified_map_err_binding_is_caught_without_receiver_type_inference() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
fn builder_failure(builder: Builder) -> Result<Value, TemporalError> {
    builder.build().map_err(|err| {
        TemporalError::new(TemporalErrorKind::InvalidDescriptor(
            InvalidDescriptorSource::new(format!("could not build: {err}")),
        ))
    })
}
fn parser_failure(input: &str) -> Result<Value, TemporalError> {
    input.parse().map_err(|cause| {
        TemporalError::new(TemporalErrorKind::ParseRejected(
            ParseRejectedSource::new("date", cause.to_string()),
        ))
    })
}
fn if_let_failure(builder: Builder) -> Result<Value, TemporalError> {
    if let Err(cause) = builder.build() {
        return Err(TemporalError::new(TemporalErrorKind::InvalidDescriptor(
            InvalidDescriptorSource::new(format!("could not build: {cause}")),
        )));
    }
    Ok(Value::default())
}
fn match_failure(builder: Builder) -> Result<Value, TemporalError> {
    match builder.build() {
        Err(cause) => Err(TemporalError::new(TemporalErrorKind::InvalidDescriptor(
            InvalidDescriptorSource::new(cause.to_string()),
        ))),
        Ok(value) => Ok(value),
    }
}
fn side_effect_does_not_retain(builder: Builder) -> Result<Value, TemporalError> {
    builder.build().map_err(|err| {
        let message = format!("could not build: {err}");
        observe(err);
        TemporalError::new(TemporalErrorKind::InvalidDescriptor(
            InvalidDescriptorSource::new(message),
        ))
    })
}
fn preserved(builder: Builder) -> Result<Value, TemporalError> {
    builder.build().map_err(|err| {
        TemporalError::new(TemporalErrorKind::Builder(
            BuilderSource::new(err, format!("builder rejected: {err}")),
        ))
    })
}
"#;
    let findings = compliance_findings(source, false)?;
    let stringify: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule_id() == InternalErrorComplianceId::StringifyForeign001)
        .collect();
    assert_eq!(stringify.len(), 5, "{findings:?}");
    assert!(
        stringify
            .iter()
            .any(|finding| finding.context().ends_with("builder_failure"))
    );
    assert!(
        stringify
            .iter()
            .any(|finding| finding.context().ends_with("parser_failure"))
    );
    assert!(
        stringify
            .iter()
            .any(|finding| finding.context().ends_with("if_let_failure"))
    );
    assert!(
        stringify
            .iter()
            .any(|finding| finding.context().ends_with("match_failure"))
    );
    assert!(
        stringify
            .iter()
            .any(|finding| finding.context().ends_with("side_effect_does_not_retain"))
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context().ends_with("preserved"))
    );
    Ok(())
}

#[test]
fn proc_macro_crate_may_terminate_the_chain_at_syn_error() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use syn::Error;

fn qualified_new(item_fn: &ItemFn) -> syn::Result<String> {
    predicate_body(item_fn).map_err(|reason| {
        syn::Error::new(item_fn.span(), format!("predicate {reason}"))
    })
}

fn bare_new_spanned(input: &DeriveInput) -> syn::Result<TokenStream> {
    ProofTypeContextBuilder::default()
        .build()
        .map_err(|error| Error::new_spanned(input, error.to_string()))
}

fn unrelated_local_error(builder: Builder) -> Result<Value, TemporalError> {
    builder.build().map_err(|err| {
        TemporalError::new(TemporalErrorKind::InvalidDescriptor(
            InvalidDescriptorSource::new(format!("could not build: {err}")),
        ))
    })
}
"#;
    let lib_findings = compliance_findings(source, true)?;
    assert!(
        !lib_findings
            .iter()
            .any(|finding| finding.context().ends_with("qualified_new")),
        "{lib_findings:?}"
    );
    assert!(
        !lib_findings
            .iter()
            .any(|finding| finding.context().ends_with("bare_new_spanned")),
        "{lib_findings:?}"
    );
    assert!(
        lib_findings.iter().any(
            |finding| finding.context().ends_with("unrelated_local_error")
                && finding.rule_id() == InternalErrorComplianceId::StringifyForeign001
        ),
        "a non-syn::Error site must still be caught in a proc-macro crate: {lib_findings:?}"
    );

    let non_proc_macro_findings = compliance_findings(source, false)?;
    assert!(
        non_proc_macro_findings
            .iter()
            .any(|finding| finding.context().ends_with("qualified_new")
                && finding.rule_id() == InternalErrorComplianceId::StringifyForeign001),
        "outside a proc-macro crate, building a syn::Error is not exempt: {non_proc_macro_findings:?}"
    );
    assert!(
        non_proc_macro_findings
            .iter()
            .any(|finding| finding.context().ends_with("bare_new_spanned")
                && finding.rule_id() == InternalErrorComplianceId::StringifyForeign001)
    );
    Ok(())
}
