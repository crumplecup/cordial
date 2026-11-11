//! Functions reached only from proof contexts: skipped, stripped from methods, and gated when ordinary.

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
fn apply_skips_function_called_only_from_proof_context() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("<Checker as Ensures>::ensures", 8)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions(),
        0,
        "Checker::ensures's only known caller is proofs::harness, itself \
         nested in #[cfg(kani)] -- apply must not write a span (gated or \
         otherwise); with no #[instrument] to strip, the file stays put"
    );
    assert_eq!(summary.skipped_policy(), 1);
    assert_eq!(summary.unresolved(), 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}

#[test]
fn apply_finds_calls_inside_a_harness_style_macro_invocation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("<Checker as Ensures>::ensures", 6)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

fake_harness_macro::harness! {
    kani, VERIFY_CHECKER_SRC, {
        #[kani::proof]
        fn verify_checker() {
            assert!(Checker::ensures(1));
        }
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions(),
        0,
        "syn never expands macros -- `fake_harness_macro::harness! {{ .. }}` \
         parses as an opaque Item::Macro, so without extracting its \
         trailing brace-block of real items, the call graph would never \
         see `#[kani::proof] fn verify_checker` at all, let alone the \
         `Checker::ensures` call inside it; the real `amenable_derive::\
         harness!` macro this is modeled on wraps almost every Kani proof \
         harness in amenable_kani the same way. Apply must not write a \
         span; with none to strip, the file stays put"
    );
    assert_eq!(summary.skipped_policy(), 1);
    assert_eq!(summary.unresolved(), 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}

#[test]
fn apply_strips_instrument_from_proof_only_method() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("<Checker as Ensures>::ensures", 9)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[tracing::instrument(level = "trace")]
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    assert_eq!(summary.unresolved(), 0, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument"),
        "proof-only Ensures::ensures must lose #[instrument], not keep a \
         not(kani) wrap that never fires: {updated}"
    );
    assert!(updated.contains("fn ensures(input: u32)"), "{updated}");
    Ok(())
}

#[test]
fn apply_strips_gated_instrument_from_proof_only_method() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("<Checker as Ensures>::ensures", 9)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[cfg_attr(not(kani), tracing::instrument(level = "trace"))]
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument") && !updated.contains("cfg_attr"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_strips_instrument_from_skip_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 3)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\n#[tracing::instrument(level = \"debug\")]\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    assert_eq!(summary.skipped_policy(), 0, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument"),
        "skip-policy apply must remove existing #[instrument]: {updated}"
    );
    assert!(updated.contains("pub fn scan_tree()"), "{updated}");
    Ok(())
}

#[test]
fn skip_policy_preserves_unrelated_attributes_containing_instrument() -> miette::Result<()> {
    cordial::init_tracing();
    for (source, expected, changed) in [
        (
            "#[doc = \"instrument\"]\npub fn scan_tree() {}\n",
            "#[doc = \"instrument\"]\npub fn scan_tree() {}\n",
            0,
        ),
        (
            "#[doc = \"instrument\"] #[tracing::instrument]\npub fn scan_tree() {}\n",
            "#[doc = \"instrument\"] \npub fn scan_tree() {}\n",
            1,
        ),
        (
            "#[cfg_attr(any(), tracing::instrument, inline)]\npub fn scan_tree() {}\n",
            "#[cfg_attr(any(), tracing::instrument, inline)]\npub fn scan_tree() {}\n",
            0,
        ),
    ] {
        let fixture = write_policy_fixture(
            &["fixture_crate"],
            "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
            &checklist_for_crate("fixture_crate", &[("scan_tree", 2)]),
        )?;
        write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;
        let summary =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()
                .wrap_err("apply skip policy")?;
        assert_eq!(
            summary.changed_functions(),
            changed,
            "{summary:?}: {source}"
        );
        let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
            .into_diagnostic()
            .wrap_err("read preserved attributes")?;
        assert_eq!(updated, expected);
    }
    Ok(())
}

#[test]
fn apply_gates_bare_instrument_on_ordinary_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 3)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\n#[tracing::instrument(level = \"debug\")]\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "{updated}"
    );
    Ok(())
}
