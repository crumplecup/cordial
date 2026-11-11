//! Functions reachable only from proof contexts are never flagged; an ordinary caller changes that.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_manifest.rs"]
mod tracing_manifest;

use tracing_manifest::write_minimal_crate_manifest;

#[test]
fn tracing_never_flags_function_nested_in_configured_gate_cfg() -> miette::Result<()> {
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
#[cfg(kani)]
mod proofs {
    pub fn proof_harness() {}
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
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        1,
        "proof_harness only exists under #[cfg(kani)], the same cfg name \
         this crate's own apply_gate_crates entry names -- #[instrument] \
         can never fire there in any real build, so it must never become \
         an open checklist item at all, only traced_ordinary_fn should"
    );

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("traced_ordinary_fn"), "{checklist}");
    assert!(!checklist.contains("proof_harness"), "{checklist}");
    Ok(())
}

#[test]
fn tracing_never_flags_function_called_only_from_proof_context() -> miette::Result<()> {
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
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        1,
        "Checker::ensures's only known caller is proofs::harness, itself \
         nested in #[cfg(kani)] -- neither is ever recorded as needing \
         #[instrument], discovered via call-graph reachability rather \
         than a hardcoded trait name; only traced_ordinary_fn should be \
         open"
    );

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(checklist.contains("traced_ordinary_fn"), "{checklist}");
    assert!(!checklist.contains("Checker"), "{checklist}");
    assert!(!checklist.contains("harness"), "{checklist}");
    Ok(())
}

#[test]
fn tracing_still_flags_function_with_an_ordinary_caller_too() -> miette::Result<()> {
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

pub fn validate(value: u32) -> bool {
    Checker::ensures(value)
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
    let open: Vec<_> = outcome
        .findings()
        .filter(|finding| finding.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(
        open.len(),
        2,
        "Checker::ensures also has a real, ordinary caller (validate), \
         so it must stay open (Gated) even though it also happens to be \
         called from proofs::harness -- a function is only ever excluded \
         once ALL its known callers are proof-only, never merely SOME of \
         them"
    );
    Ok(())
}
