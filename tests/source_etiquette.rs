use cordial::{
    EtiquetteExplain, EtiquetteHooks, RunAll, ScopeEnricher, Session, SessionBuilder, SourceLoader,
    StaticEtiquette, project_slug_from_path,
};
use miette::{IntoDiagnostic, WrapErr};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;

static LOADERS: &[&'static dyn cordial::Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&'static dyn cordial::IrEnricher] = &[&SCOPE_ENRICHER];

static SOURCE_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
    "source",
    "Source inventory",
    EtiquetteHooks::new(LOADERS, ENRICHERS, &[], &[], None, &[]),
    false,
    EtiquetteExplain::new(
        "Test inventory (not a product lint)",
        "Session fixture used by cordial's own tests.",
        "Loads source (and optionally rustdoc) into IR; emits no findings.",
        "Not registered in the cordial binary.",
        &[],
    ),
);

#[test]
fn source_loader_builds_ir_and_cache() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
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
        .register(&SOURCE_ETIQUETTE)
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

    let slug = project_slug_from_path(fixture.path());
    let cache_path = store.path().join("cache").join(format!("{slug}.ir.json"));
    assert!(
        cache_path.is_file(),
        "expected IR cache at {}",
        cache_path.display()
    );
    Ok(())
}
