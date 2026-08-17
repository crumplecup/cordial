use cordial::{
    NamedRunFilter, Plugin, PluginCategory, Session, SessionBuilder, plugins_in_category,
};
use miette::{IntoDiagnostic, WrapErr};

use cordial_custom_plugins::{ACME_API_COVERAGE, ACME_ERROR_HANDLING, ACME_STYLE};

#[test]
fn three_plugin_kinds_register_and_quality_finds_todo() -> miette::Result<()> {
    let plugins: Vec<&dyn Plugin> = vec![&ACME_STYLE, &ACME_API_COVERAGE, &ACME_ERROR_HANDLING];

    let quality = plugins_in_category(&plugins, PluginCategory::Quality);
    assert_eq!(quality.len(), 1);
    assert_eq!(quality[0].id(), "acme-style");

    let coverage = plugins_in_category(&plugins, PluginCategory::Coverage);
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0].id(), "acme-api-coverage");

    let error_handling = plugins_in_category(&plugins, PluginCategory::ErrorHandling);
    assert_eq!(error_handling.len(), 1);
    assert_eq!(error_handling[0].id(), "acme-error-handling");

    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    std::fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn leftover() {\n    todo!(\"wire this up\")\n}\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register_plugin(&ACME_STYLE)
        .register_plugin(&ACME_API_COVERAGE)
        .register_plugin(&ACME_ERROR_HANDLING)
        .build();

    // Coverage reuses IMPL_COVERAGE_ETIQUETTE, which needs rustdoc JSON. A
    // fixture has none, so the run filters to the two source-scan families.
    let filter = NamedRunFilter::plugins(&["acme-style", "acme-error-handling"]);
    let outcome = session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule().id() == "ACME-TODO-001"),
        "planted todo!() should produce ACME-TODO-001"
    );
    Ok(())
}
