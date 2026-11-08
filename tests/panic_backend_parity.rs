//! Native panic collection preserves the complete policy and snapshot payload.
#![cfg(feature = "panics")]
mod support {
    pub mod semantic_metrics;
}

use cordial::{
    CrateIr, CrateTarget, IrView, LoadContext, Loader, SessionBuilder, SourceDatabase,
    SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::prelude::*;

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_panics_preserve_complete_policy_across_revisions() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(metrics.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='panic_parity'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let source = include_str!("fixtures/panics/graph_root.rs").to_string();
    let mut modules = String::new();
    for index in 1..=28 {
        let name = format!("sample_{index:02}");
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/panics")
                .join(format!("{name}.rs")),
        )
        .into_diagnostic()?;
        std::fs::write(root.join("src").join(format!("{name}.rs")), text).into_diagnostic()?;
        modules.push_str(&format!("\nmod {name};"));
    }
    let source = format!("{source}{modules}");
    let path = root.join("src/lib.rs");
    std::fs::write(&path, &source).into_diagnostic()?;
    std::fs::write(
        root.join("src/leaf.rs"),
        include_str!("fixtures/panics/graph_leaf.rs"),
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/middle.rs"),
        include_str!("fixtures/panics/graph_middle.rs"),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// empty evidence\n").into_diagnostic()?;
    std::fs::create_dir_all(root.join("tests/fixtures")).into_diagnostic()?;
    std::fs::write(
        root.join("tests/fixtures/ignored.rs"),
        include_str!("fixtures/panics/sample_27.rs"),
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("panic_parity", root);
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let loader = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut baseline = CrateIr::new("panic_parity");
    loader.populate_ir(&mut baseline).into_diagnostic()?;
    let expected = baseline.source_panics().into_diagnostic()?;
    let mut database = SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_panics().into_diagnostic()?;
    assert_eq!(
        serde_json::to_value(&actual).into_diagnostic()?,
        serde_json::to_value(&expected).into_diagnostic()?,
        "every snapshot, proof root, call edge, panic record and context must match"
    );
    let counts = metrics.snapshot();
    assert!(
        counts
            .get("rust_program:payload")
            .is_some_and(|count| *count > 0)
    );
    database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(
        metrics.snapshot(),
        counts,
        "unchanged projections reuse macro and embedded-program grammar"
    );
    let changed = source
        .replace("#[kani::proof]", "#[other::proof]")
        .replace("manual repair", "changed repair");
    database.change_file(&path, changed).into_diagnostic()?;
    let revised = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_panics()
        .into_diagnostic()?;
    assert_ne!(actual, revised);
    assert!(!revised.proof_graph().roots().contains("root"));
    assert!(
        revised
            .records()
            .iter()
            .any(|record| record.snippet().contains("changed repair"))
    );
    assert_eq!(std::fs::read_to_string(&path).into_diagnostic()?, source);
    database.change_file(&path, source).into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_panics()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&path, "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid outer source cannot issue complete panic evidence"
    );
    drop(database);
    drop(loaded);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        native.source_panics().into_diagnostic()?,
        actual,
        "consumption needs only persisted IR"
    );
    Ok(())
}
