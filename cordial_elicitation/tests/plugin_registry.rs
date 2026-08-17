#![cfg(feature = "elicitation")]

use cordial::{Coverage, Plugin, RunAll, Session, SessionBuilder};
use cordial_elicitation::{ELICITATION_COVERAGE, ELICITATION_TRACKED_TARGETS, ElicitationCoverage};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn elicitation_tracked_targets_roster_is_non_empty() {
    assert!(!ELICITATION_TRACKED_TARGETS.is_empty());
    assert!(
        ELICITATION_TRACKED_TARGETS
            .iter()
            .any(|target| target.upstream == "url")
    );
}

#[test]
fn elicitation_coverage_plugin_wires_three_etiquettes() {
    let plugin = &ELICITATION_COVERAGE;
    assert_eq!(plugin.id(), "elicitation-coverage");
    assert_eq!(plugin.etiquettes().len(), 3);
    assert!(
        plugin
            .etiquettes()
            .iter()
            .any(|e| e.id() == "impl-coverage")
    );
    assert!(plugin.etiquettes().iter().any(|e| e.id() == "trenchcoat"));
    assert!(plugin.etiquettes().iter().any(|e| e.id() == "shadow"));
}

#[test]
fn elicitation_coverage_targets_match_workspace_members() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    std::fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    std::fs::write(fixture.path().join("src/lib.rs"), "pub struct Widget;")
        .into_diagnostic()
        .wrap_err("write")?;
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()
    .wrap_err("manifest")?;

    let session = SessionBuilder::new(fixture.path()).build();
    let targets = ElicitationCoverage
        .targets(&session, &RunAll)
        .into_diagnostic()
        .wrap_err("targets")?;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].crate_name, "demo");
    Ok(())
}

#[test]
fn register_plugin_runs_same_as_individual_etiquettes() -> miette::Result<()> {
    use std::fs;

    use cordial::rustdoc::{demo_impl_coverage_crate, write_rustdoc_crate_json};

    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("demo");
    fs::create_dir_all(crate_root.join("doc"))
        .into_diagnostic()
        .wrap_err("doc")?;
    write_rustdoc_crate_json(
        &crate_root.join("doc/demo.json"),
        &demo_impl_coverage_crate(),
    )
    .into_diagnostic()
    .wrap_err("json")?;

    let store_plugin = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let outcome_plugin = SessionBuilder::new(&crate_root)
        .with_store_root(store_plugin.path())
        .register_plugin(&ELICITATION_COVERAGE)
        .build()
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("plugin run")?;

    let open_plugin = outcome_plugin
        .findings()
        .filter(|f| f.disposition() == cordial::Disposition::Open)
        .count();
    assert!(open_plugin >= 1);
    assert!(
        store_plugin
            .path()
            .join("findings/impl-coverage.csv")
            .is_file()
    );
    assert!(
        store_plugin
            .path()
            .join("findings/trenchcoats.csv")
            .is_file()
    );
    Ok(())
}
