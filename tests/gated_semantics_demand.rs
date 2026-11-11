//! Cfg-gated code is analysed only when a selected etiquette reads its facts.

use cordial::{
    CrateIr, IrView, PANICS_ETIQUETTE, RunAll, Session, SessionBuilder, TRACING_ETIQUETTE,
};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = "\
pub fn always() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(feature = \"alternate\")]
pub fn gated() -> Result<(), std::io::Error> { Ok(()) }
";

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

/// The context plan recorded by a run of `session` over the fixture.
fn plan_of(
    session: cordial::RuntimeSession,
    store: &std::path::Path,
) -> Result<cordial::SemanticContextPlan> {
    session.run(&RunAll).into_diagnostic()?;
    let ir = CrateIr::read_cache(&store.join("cache/fixture.ir.zst")).into_diagnostic()?;
    ir.semantic_context_plan()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing context plan"))
}

#[test]
fn an_etiquette_that_ignores_gated_code_pays_nothing_for_it() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(root.path())
        .with_store_root(store.path())
        .with_ir_snapshot()
        .register(&*PANICS_ETIQUETTE)
        .build();
    let plan = plan_of(session, store.path())?;
    assert!(plan.contexts().is_empty() && *plan.complete(), "{plan:?}");
    Ok(())
}

#[test]
fn tracing_asks_for_the_contexts_that_activate_gated_functions() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(root.path())
        .with_store_root(store.path())
        .with_ir_snapshot()
        .register(&*TRACING_ETIQUETTE)
        .build();
    let plan = plan_of(session, store.path())?;
    let contexts: Vec<_> = plan.contexts().iter().map(ToString::to_string).collect();
    assert_eq!(contexts, ["+feature=\"alternate\""]);
    assert!(*plan.complete());
    Ok(())
}
