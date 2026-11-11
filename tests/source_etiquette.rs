use std::sync::LazyLock;

use cordial::{
    EtiquetteExplain, EtiquetteHooks, RunAll, ScopeEnricher, Session, SessionBuilder, SourceLoader,
    StaticEtiquette,
};
use miette::{IntoDiagnostic, WrapErr};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;

static LOADERS: &[&'static dyn cordial::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn cordial::IrEnricher] = &[&SCOPE_ENRICHER];

static SOURCE_ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "source",
        "Source inventory",
        EtiquetteHooks::new(LOADERS, ENRICHERS, &[], &[], None, &[]),
        false,
        EtiquetteExplain::new(
            "Test inventory (not a product lint)",
            "Session fixture used by cordial's own tests.",
            "Loads source (and optionally rustdoc) into IR; emits no findings.",
            "Not registered in the cordial binary.",
            vec![],
        ),
    )
});

#[test]
fn source_loader_builds_ir_and_cache() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn hello() {}\n\npub struct Widget;",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*SOURCE_ETIQUETTE)
        .with_ir_snapshot()
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 0);
    #[cfg(feature = "quality")]
    assert_eq!(outcome.artifacts().count(), 3);
    #[cfg(not(feature = "quality"))]
    assert_eq!(outcome.artifacts().count(), 1);
    assert!(
        outcome
            .artifacts()
            .any(|artifact| artifact.name() == "rollup-summary.md")
    );
    #[cfg(feature = "quality")]
    {
        assert!(
            outcome
                .artifacts()
                .any(|artifact| artifact.name() == "quality-report.md")
        );
        assert!(
            outcome
                .artifacts()
                .any(|artifact| artifact.name() == "summary.md")
        );
    }

    let slug = "fixture";
    let cache_path = store.path().join("cache").join(format!("{slug}.ir.zst"));
    assert!(
        cache_path.is_file(),
        "expected IR cache at {}",
        cache_path.display()
    );
    Ok(())
}
