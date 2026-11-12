//! A function is proof-only only if every caller is, and an `#[instrument]`ed
//! caller is a caller like any other: its calls are not hidden by the macro.

use cordial::{RunAll, Session, SessionBuilder, TRACING_ETIQUETTE};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

const LIBRARY: &str = include_str!("fixtures/tracing_instrumented_callers/lib.rs.txt");

#[test]
fn a_function_called_by_an_instrumented_caller_is_not_proof_only() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture_crate'\nversion = '0.1.0'\nedition = '2024'\n[dependencies]\ntracing = '0.1'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*TRACING_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("tracing-instrument.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(
        checklist.contains("shared"),
        "`shared` has an ordinary caller, so it needs instrumenting:\n{checklist}"
    );
    assert!(!checklist.contains("harness"), "{checklist}");
    Ok(())
}
