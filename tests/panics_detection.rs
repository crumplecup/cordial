//! Which panic sites are found and which are not: macros, `expect`/`unwrap` chains, `expect_err` exemptions, and nested workspaces.

use cordial::{PANICS_ETIQUETTE, PanicKind, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/panic_sites.rs"]
mod panic_sites;

#[test]
fn panics_etiquette_detects_panic_expect_and_unreachable() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_01.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .with_ir_snapshot()
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 4);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("panics.csv"))
        .into_diagnostic()
        .wrap_err("panics csv")?;
    assert!(csv.contains("PANIC-SOURCE-PANIC"));
    assert!(csv.contains("PANIC-SOURCE-EXPECT"));
    assert!(csv.contains("PANIC-SOURCE-UNREACHABLE"));

    let checklist = fs::read_to_string(findings_dir.join("panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("panic!"));
    assert!(checklist.contains("Library — return internal error types"));
    assert!(checklist.contains("internal error type"));

    let summary = fs::read_to_string(findings_dir.join("panics-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("| 4 | 1 | 1 | 1 | 1 | 0 |"));
    assert!(summary.contains("unwrap **1**"));

    let slug = "fixture";
    assert!(
        store
            .path()
            .join("cache")
            .join(format!("{slug}.ir.zst"))
            .is_file()
    );
    Ok(())
}

#[test]
fn panics_disabled_in_cordial_toml_skips_the_etiquette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_20.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[panics]\nenabled = false\n",
    )
    .into_diagnostic()
    .wrap_err("cordial.toml")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 0);
    assert!(
        !store.path().join("findings/panics.csv").is_file(),
        "disabled etiquette must not write findings"
    );
    Ok(())
}

#[test]
fn finds_compile_error() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_05.rs"))?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind(), PanicKind::CompileError);
    Ok(())
}

#[test]
fn finds_abort_sites_inside_a_string_that_parses_as_rust() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/embedded_host.rs"))?;
    assert_eq!(
        findings.len(),
        1,
        "a string that parses as Rust and contains panic! is an embedded fixture: {findings:?}"
    );
    assert_eq!(findings[0].kind(), PanicKind::Panic);
    Ok(())
}

#[test]
fn does_not_flag_panic_in_an_assertion_message() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/assertion_message.rs"))?;
    assert!(
        findings.is_empty(),
        "\"panic!\" as a message does not parse as a Rust file: {findings:?}"
    );
    Ok(())
}

#[test]
fn chained_unwrap_on_one_line_is_one_site() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_07.rs"))?;
    let unwraps = findings
        .iter()
        .filter(|row| row.kind() == PanicKind::Unwrap)
        .count();
    assert_eq!(
        unwraps, 1,
        "chained unwraps on one line should be one site: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_err_and_expect_err_are_flagged_the_same_as_their_ok_counterparts() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_08.rs"))?;

    let unwraps = findings
        .iter()
        .filter(|row| row.kind() == PanicKind::Unwrap)
        .count();
    let expects = findings
        .iter()
        .filter(|row| row.kind() == PanicKind::Expect)
        .count();
    assert_eq!(unwraps, 1, "{findings:?}");
    assert_eq!(expects, 1, "{findings:?}");
    assert!(
        findings.iter().any(|row| row.snippet() == ".unwrap_err()"),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|row| row.snippet() == ".expect_err(\"must be an error\")"),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_asserted_against_directly_is_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_kani::ledger_test shape: the whole point of the call
    // is to extract the error value and assert a fact about it, not to
    // discard or propagate a setup failure.
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_09.rs"))?;
    assert!(
        findings.is_empty(),
        "error is bound then asserted on directly -- must be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_exemption_matches_even_with_a_multiline_receiver() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable main.rs shape: check_method_call computes its own
    // exempt-lookup line from the whole call expression's span, which
    // starts at the *receiver*'s own beginning for a multi-line
    // receiver -- not the .expect_err( call's own line. Both line
    // computations must agree, or the lookup silently misses (confirmed
    // the hard way: this exact fixture's real-world counterpart stayed
    // flagged until error_assertion_binding's own line computation was
    // fixed to match).
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_10.rs"))?;
    assert!(
        findings.is_empty(),
        "error is bound then asserted on directly, receiver just spans several lines first -- \
         must still be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_err_asserted_against_one_hop_later_is_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    // Real assessment_error_chain_test shape: the bound error feeds a
    // further `let`, and THAT is what's asserted on.
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_11.rs"))?;
    assert!(
        findings.is_empty(),
        "error flows into rendered one hop later, which IS asserted on -- must be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_with_no_later_use_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_12.rs"))?;
    assert_eq!(
        findings.len(),
        1,
        "bound but never used again -- not an assertion pattern, must still be flagged: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_bound_and_used_but_never_asserted_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_13.rs"))?;
    assert_eq!(
        findings.len(),
        1,
        "used, but never inside a real assert*! -- must still be flagged: {findings:?}"
    );
    Ok(())
}

#[test]
fn nested_parity_workspace_is_skipped_when_scanning_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests/parity/workspaces/child/src"))
        .into_diagnostic()
        .wrap_err("nested")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_28.rs"),
    )
    .into_diagnostic()
    .wrap_err("parent lib")?;
    fs::write(
        fixture
            .path()
            .join("tests/parity/workspaces/child/src/lib.rs"),
        include_str!("fixtures/panics/sample_27.rs"),
    )
    .into_diagnostic()
    .wrap_err("nested lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("parent"),
        "parent abort site should scan: {csv}"
    );
    assert!(
        !csv.contains("nested fixture"),
        "tests/parity under the scanned crate is not production source: {csv}"
    );
    Ok(())
}
