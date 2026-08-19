use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::rustdoc::{
    demo_impl_coverage_crate, demo_shadow_crate, demo_trenchcoat_crate, write_rustdoc_crate_json,
};
use cordial::{
    IMPL_COVERAGE_ETIQUETTE, Plugin, RunAll, SHADOW_ETIQUETTE, Session, SessionBuilder,
    TRENCHCOAT_ETIQUETTE,
};

fn write_fixture(
    parent: &std::path::Path,
    krate: rustdoc_types::Crate,
) -> miette::Result<std::path::PathBuf> {
    let crate_root = parent.join("demo");
    fs::create_dir_all(crate_root.join("doc"))
        .into_diagnostic()
        .wrap_err("doc dir")?;
    write_rustdoc_crate_json(&crate_root.join("doc/demo.json"), &krate)
        .into_diagnostic()
        .wrap_err("write json")?;
    Ok(crate_root)
}

#[test]
fn impl_coverage_etiquette_finds_missing_traits() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = write_fixture(fixture.path(), demo_impl_coverage_crate())?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(&crate_root)
        .with_store_root(store.path())
        .register(&IMPL_COVERAGE_ETIQUETTE)
        .build();

    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let open: Vec<_> = outcome
        .findings()
        .filter(|f| f.disposition() == cordial::Disposition::Open)
        .collect();
    assert_eq!(open.len(), 1, "Widget missing ElicitComplete prerequisites");

    let csv = fs::read_to_string(store.path().join("findings/impl-coverage.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("demo::Widget"));
    assert!(csv.contains("MissingOurTraits"));
    assert!(store.path().join("findings/gaps-impl.csv").is_file());
    Ok(())
}

#[test]
fn trenchcoat_etiquette_finds_unwrapped_foreign_type() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = write_fixture(fixture.path(), demo_trenchcoat_crate())?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(&crate_root)
        .with_store_root(store.path())
        .register(&TRENCHCOAT_ETIQUETTE)
        .build();

    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings.iter().any(|finding| {
            let mut sink = cordial::MapFindingSink::default();
            finding.emit(&mut sink);
            sink.fields
                .iter()
                .any(|(_, value)| value.contains("BareForeign"))
        }),
        "expected BareForeign to lack a wrapper"
    );
    Ok(())
}

#[test]
fn shadow_etiquette_links_mapped_items() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = write_fixture(fixture.path(), demo_shadow_crate())?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(&crate_root)
        .with_store_root(store.path())
        .register(&SHADOW_ETIQUETTE)
        .build();

    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert_eq!(
        outcome
            .findings()
            .filter(|f| f.disposition() == cordial::Disposition::Open)
            .count(),
        0
    );

    let cache = fs::read_to_string(store.path().join("cache").join(format!(
        "{}.ir.json",
        cordial::project_slug_from_path(&crate_root)
    )))
    .into_diagnostic()
    .wrap_err("cache")?;
    assert!(cache.contains("Mirrors"));
    Ok(())
}

#[test]
fn elicitation_tracked_targets_roster_is_non_empty() {
    assert!(!cordial::ELICITATION_TRACKED_TARGETS.is_empty());
    assert!(
        cordial::ELICITATION_TRACKED_TARGETS
            .iter()
            .any(|target| target.upstream == "url")
    );
}

#[test]
fn elicitation_coverage_plugin_wires_three_etiquettes() {
    let plugin = &cordial::ELICITATION_COVERAGE;
    assert_eq!(plugin.id(), "elicitation-coverage");
    assert_eq!(plugin.etiquettes().len(), 3);
}
