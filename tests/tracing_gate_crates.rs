//! Gate crates: `cfg_attr`-gated `#[instrument]`, the gated wrap in the checklist, and `skip_crate`.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_manifest.rs"]
mod tracing_manifest;

use tracing_manifest::write_minimal_crate_manifest;

#[test]
fn tracing_treats_cfg_attr_gated_instrument_as_present() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[cfg_attr(not(kani), tracing::instrument(level = "debug"))]
pub fn scan_tree() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        0,
        "apply writes #[cfg_attr(not(kani), tracing::instrument(..))] for \
         gated crates; a later quality run must treat that as present, not \
         as a missing-instrument (or recipe-delta) gap"
    );
    Ok(())
}

fn open_rule_ids(outcome: &dyn cordial::RunOutcome) -> Vec<String> {
    outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .map(|finding| finding.rule().id().to_string())
        .collect()
}

#[test]
fn tracing_checklist_recipe_shows_the_gated_wrap_for_a_gate_crate() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "\npub fn scan_tree() {}\n",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(
        checklist.contains("`#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]`"),
        "the recipe cell must show the exact gated attribute `--apply` writes for a \
         gate-policy crate, not the bare `#[instrument(..)]` a human would then paste \
         into a file a `cargo kani` build compiles: {checklist}"
    );
    assert!(
        !checklist.contains("`#[instrument(level = \"debug\")]`"),
        "no bare-attr recipe should survive for a gate crate: {checklist}"
    );
    Ok(())
}

#[test]
fn tracing_proof_only_method_with_instrument_is_flagged() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
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

pub fn traced_ordinary_fn() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert!(
        rules.iter().any(|id| id == "TRACING-PROOF-INSTRUMENT"),
        "Ensures::ensures is only called from a kani harness — the span \
         must come off, not stay as a missing-instrument or gated recipe: {rules:?}"
    );
    assert!(
        rules.iter().any(|id| id == "TRACING-MISSING-INSTRUMENT"),
        "ordinary traced_ordinary_fn still wants a span: {rules:?}"
    );
    assert_eq!(rules.len(), 2, "{rules:?}");
    Ok(())
}

#[test]
fn tracing_proof_only_gated_instrument_never_fires_so_it_is_flagged() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[cfg_attr(not(kani), tracing::instrument(level = "trace"))]
    fn ensures(input: T) -> bool {
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
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-PROOF-INSTRUMENT"],
        "gating not(kani) on a proof-only Ensures impl is a dead span — \
         the method never runs outside the prover: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_ungated_instrument_on_ordinary_fn_in_gate_crate() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn scan_tree() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-UNGATED-INSTRUMENT"],
        "bare #[instrument] on ordinary code in a kani crate is visible \
         to the prover; attenuation is gate, not remove: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_skip_crate_with_instrument_is_flagged() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
#[tracing::instrument(level = "debug")]
pub fn scan_tree() {}

pub fn other_fn() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert_eq!(
        rules.as_slice(),
        ["TRACING-SKIP-INSTRUMENT"],
        "skip-policy (Verus/Creusot) must not keep #[instrument]; \
         uninstrumented other_fn must not get a missing-instrument push: {rules:?}"
    );
    Ok(())
}

#[test]
fn tracing_skip_crate_uninstrumented_is_silent() -> miette::Result<()> {
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
    write_minimal_crate_manifest(fixture.path(), "fixture_crate")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "\npub fn scan_tree() {}\n",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let rules = open_rule_ids(outcome.as_ref());
    assert!(
        rules.is_empty(),
        "skip-policy with no span: do not push toward adding one: {rules:?}"
    );
    Ok(())
}
