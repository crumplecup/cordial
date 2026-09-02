use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    BoundaryRuleId, Disposition, RunAll, Session, SessionBuilder, TRACING_ETIQUETTE,
    TracingBoundaryPolicy, scan_crate_tracing_boundary,
};

fn write_bin(main: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/main.rs"), main)
        .into_diagnostic()
        .wrap_err("write main")?;
    Ok(fixture)
}

fn scan(
    root: &Path,
    crate_name: &str,
    skip_program_lints: bool,
) -> miette::Result<Vec<BoundaryRuleId>> {
    let records = scan_crate_tracing_boundary(
        root,
        crate_name,
        &TracingBoundaryPolicy::default(),
        skip_program_lints,
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id).collect())
}

fn has(rules: &[BoundaryRuleId], rule: BoundaryRuleId) -> bool {
    rules.contains(&rule)
}

#[test]
fn silent_fallible_main_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() -> Result<(), String> { Ok(()) }\n")?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, BoundaryRuleId::MainSilent),
        "expected MAIN-SILENT: {rules:?}"
    );
    Ok(())
}

#[test]
fn infallible_main_is_clean() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() {}\n")?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "main that can't return Err has nothing to report: {rules:?}"
    );
    Ok(())
}

#[test]
fn instrument_err_on_main_satisfies_the_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin(
        r#"
#[tracing::instrument(level = "info", err(level = "warn"))]
fn main() -> Result<(), String> {
    Ok(())
}
"#,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "instrument(err(...)) on main satisfies the rule: {rules:?}"
    );
    Ok(())
}

#[test]
fn bare_instrument_err_on_main_satisfies_the_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin(
        r#"
#[tracing::instrument(err)]
fn main() -> Result<(), String> {
    Ok(())
}
"#,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "bare err (no level override) still satisfies the rule: {rules:?}"
    );
    Ok(())
}

#[test]
fn direct_tracing_error_emission_satisfies_the_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin(
        r#"
fn main() -> Result<(), String> {
    if let Err(err) = run() {
        tracing::error!(error = %err, "run failed");
        return Err(err);
    }
    Ok(())
}
fn run() -> Result<(), String> {
    Ok(())
}
"#,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "explicit tracing::error! on the error path satisfies the rule: {rules:?}"
    );
    Ok(())
}

#[test]
fn delegating_to_a_reporting_helper_satisfies_the_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin(
        r#"
#[tracing::instrument(err(level = "warn"))]
fn dispatch() -> Result<(), String> {
    Ok(())
}
fn main() -> Result<(), String> {
    dispatch()
}
"#,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        rules.is_empty(),
        "main delegating to an already-reporting helper satisfies the rule: {rules:?}"
    );
    Ok(())
}

#[test]
fn delegating_to_a_silent_helper_is_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin(
        r#"
fn dispatch() -> Result<(), String> {
    Ok(())
}
fn main() -> Result<(), String> {
    dispatch()
}
"#,
    )?;
    let rules = scan(fixture.path(), "fixture", false)?;
    assert!(
        has(&rules, BoundaryRuleId::MainSilent),
        "the delegate never reports either, so this must still be flagged: {rules:?}"
    );
    Ok(())
}

#[test]
fn known_cross_crate_helper_is_trusted() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() -> Result<(), String> { other_crate::run() }\n")?;
    let policy = TracingBoundaryPolicy::new(true, vec!["other_crate::run".to_string()]);
    let records = scan_crate_tracing_boundary(fixture.path(), "fixture", &policy, false)
        .into_diagnostic()
        .wrap_err("scan")?;
    let rules: Vec<_> = records.into_iter().map(|record| record.rule_id).collect();
    assert!(
        rules.is_empty(),
        "a configured cross-crate helper is trusted, same as subscriber's known_helper_paths: {rules:?}"
    );
    Ok(())
}

#[test]
fn knob_off_is_silent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() -> Result<(), String> { Ok(()) }\n")?;
    let policy = TracingBoundaryPolicy::new(false, Vec::new());
    let records = scan_crate_tracing_boundary(fixture.path(), "fixture", &policy, false)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records.is_empty(),
        "main_reports_errors = false must be silent"
    );
    Ok(())
}

#[test]
fn skip_program_lints_silences_main() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() -> Result<(), String> { Ok(()) }\n")?;
    let rules = scan(fixture.path(), "fixture", true)?;
    assert!(
        !has(&rules, BoundaryRuleId::MainSilent),
        "skip-policy crate skips MAIN-SILENT: {rules:?}"
    );
    Ok(())
}

#[test]
fn session_writes_boundary_checklist_not_instrument_rows() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_bin("fn main() -> Result<(), String> { Ok(()) }\n")?;
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let boundary: Vec<_> = outcome
        .findings()
        .filter(|finding| {
            finding.disposition() == Disposition::Open
                && BoundaryRuleId::is_boundary_rule(finding.rule().id())
        })
        .collect();
    assert!(
        boundary
            .iter()
            .any(|finding| finding.rule().id() == BoundaryRuleId::MainSilent.as_str()),
        "session should emit MAIN-SILENT"
    );

    let findings_dir = store.path().join("findings");
    let instrument = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("instrument checklist")?;
    assert!(
        !instrument.contains("TRACING-BOUNDARY-"),
        "instrument checklist must not swallow boundary rows: {instrument}"
    );
    let checklist = fs::read_to_string(findings_dir.join("tracing-boundary.checklist.md"))
        .into_diagnostic()
        .wrap_err("boundary checklist")?;
    assert!(checklist.contains("TRACING-BOUNDARY-MAIN-SILENT"));
    assert!(checklist.contains("**Open items:**"));
    Ok(())
}

#[test]
fn dogfood_cordial_boundary_policy_is_clean() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = scan(root, "cordial", false)?;
    assert!(
        rules.is_empty(),
        "cordial should satisfy its own binary error-boundary policy: {rules:?}"
    );
    Ok(())
}
