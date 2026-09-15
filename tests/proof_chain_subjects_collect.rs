//! `collect_proof_chain_subjects` must find `proof_chain("...")` calls even
//! when rustfmt wraps them across multiple lines — a long subject (e.g. a
//! jiff type with a long name) pushes the call past rustfmt's width limit,
//! landing the opening quote on its own line rather than right after the
//! paren.

#![cfg(feature = "amenable_ext")]

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::testing::collect_proof_chain_subjects;

#[test]
fn collect_proof_chain_subjects_finds_a_single_line_call() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let harness_dir = dir.path().join("crates/amenable/tests");
    fs::create_dir_all(&harness_dir)
        .into_diagnostic()
        .wrap_err("mkdir")?;
    fs::write(
        harness_dir.join("proof_chain_test.rs"),
        r#"
            fn ext_offset_proof_chain_reports_all_three_verifiers() {
                let report = support::chain(amenable::proof_chain("ExtStandard<jiff::tz::Offset>"))?;
            }
        "#,
    )
    .into_diagnostic()
    .wrap_err("write harness")?;

    let subjects = collect_proof_chain_subjects(dir.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(subjects.contains("ExtStandard<jiff::tz::Offset>"));
    Ok(())
}

#[test]
fn collect_proof_chain_subjects_finds_a_rustfmt_wrapped_call() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let harness_dir = dir.path().join("crates/amenable/tests");
    fs::create_dir_all(&harness_dir)
        .into_diagnostic()
        .wrap_err("mkdir")?;
    fs::write(
        harness_dir.join("proof_chain_test.rs"),
        r#"
            fn ext_signed_duration_round_proof_chain_reports_all_three_verifiers() {
                let report = support::chain(amenable::proof_chain(
                    "ExtStandard<jiff::SignedDurationRound>",
                ))?;
            }
        "#,
    )
    .into_diagnostic()
    .wrap_err("write harness")?;

    let subjects = collect_proof_chain_subjects(dir.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(subjects.contains("ExtStandard<jiff::SignedDurationRound>"));
    Ok(())
}

#[test]
fn collect_proof_chain_subjects_does_not_confuse_the_two_functions() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let harness_dir = dir.path().join("crates/amenable/tests");
    fs::create_dir_all(&harness_dir)
        .into_diagnostic()
        .wrap_err("mkdir")?;
    fs::write(
        harness_dir.join("proof_chain_test.rs"),
        r#"
            fn a() {
                amenable::proof_chain("A");
            }
            fn b() {
                amenable::proof_chain_for_verifiers("B");
            }
        "#,
    )
    .into_diagnostic()
    .wrap_err("write harness")?;

    let subjects = collect_proof_chain_subjects(dir.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(subjects.contains("A"));
    assert!(subjects.contains("B"));
    assert_eq!(subjects.len(), 2);
    Ok(())
}
