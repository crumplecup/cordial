//! Gated and skipped crates: configured gates, dependency crates, spliced files, and ancestor `cfg`s.

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_policy.rs"]
mod tracing_apply_policy;

use tracing_apply_checklist::checklist_for_crate;
use tracing_apply_policy::{write_policy_crate, write_policy_fixture};

#[test]
fn apply_gates_instrument_for_configured_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 2)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);
    assert_eq!(summary.skipped_policy(), 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("use tracing::instrument;"),
        "a gated attribute is always fully qualified, so the plain import \
         would go unused whenever every applied gap in the file is gated \
         -- real precedent: amenable_kani has Kani-proof-only files where \
         this exact import went `unused_imports`-flagged: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_configured_crate_leaving_checklist_open() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 2)]),
    )?;
    let source = "\npub fn scan_tree() {}\n";
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 0);
    assert_eq!(summary.changed_files(), 0);
    assert_eq!(summary.skipped_policy(), 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "skipped file must be left byte-identical");
    Ok(())
}

#[test]
fn apply_gates_dependency_crate_via_transitive_dependent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_core", "fixture_kani"],
        "[tracing]\napply_gate_crates = { fixture_kani = \"kani\" }\n",
        &checklist_for_crate("fixture_core", &[("scan_tree", 2)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_core",
        "\npub fn scan_tree() {}\n",
        &[],
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_kani",
        "\npub fn noop() {}\n",
        &[("fixture_core", "../fixture_core")],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_core/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "fixture_core compiles as part of fixture_kani's own `cargo kani` build, so it needs \
         the same gate even though nothing in fixture_core's own config names it: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_file_spliced_into_skip_configured_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_owner", "fixture_verus"],
        "[tracing]\napply_skip_crates = [\"fixture_verus\"]\n",
        &checklist_for_crate("fixture_owner", &[("scan_tree", 2)]),
    )?;
    let owner_source = "\npub fn scan_tree() {}\n";
    write_policy_crate(&fixture.workspace, "fixture_owner", owner_source, &[])?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_verus",
        "#[path = \"../../fixture_owner/src/lib.rs\"]\nmod owner_impl;\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 0);
    assert_eq!(summary.skipped_policy(), 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_owner/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(
        updated, owner_source,
        "fixture_verus splices this exact file in via #[path], so it must stay untouched \
         even though fixture_owner itself isn't in apply_skip_crates: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_function_nested_in_ancestor_gate_cfg() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("proof_harness", 4)]),
    )?;
    let source = r#"
#[cfg(kani)]
mod proofs {
    pub fn proof_harness() {}
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions(),
        0,
        "proof_harness only exists at all under `#[cfg(kani)]`, and Gated \
         policy already suppresses `#[instrument]` whenever `kani` *is* \
         active -- so #[instrument] can never fire in any real build; the \
         checklist row can't be resolved to a recorded function at all \
         (the scanner never records it in the first place), not merely \
         applied with a qualified path"
    );
    assert_eq!(summary.unresolved(), 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}
