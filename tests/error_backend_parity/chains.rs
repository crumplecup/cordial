//! Loader-owned chain records survive without source files or parse carriers.

use super::{projection, shared};
use cordial::SourceDatabase;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

pub const PACKAGE: &str = "chain_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("tests/fixtures")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    let source = include_str!("../fixtures/semantic_backend/error_chains.rs.txt");
    let library = root.join("src/lib.rs");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(
        root.join("src/child.rs"),
        "pub fn leaf() -> Result<(), Error> { std::fs::read(\"x\")?; Ok(()) }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no chain records\n").into_diagnostic()?;
    std::fs::write(root.join("tests/fixtures/ignored.rs"), "fn invalid( {").into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let source = include_str!("../fixtures/semantic_backend/error_chains.rs.txt");
    let library = root.join("src/lib.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_chains().into_diagnostic()?;
    assert_eq!(expected.files().len(), 3);
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_chains().into_diagnostic()?;
    assert_eq!(
        actual, expected,
        "complete records, snapshots and empty-file coverage"
    );
    let hooks = &["error-site-inventory", "error-chain-inventory"];
    let rows = projection::materialized_with(&native, root, hooks, "error_chain_rule_id")?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, hooks, "error_chain_rule_id")?
    );
    for context in [
        "child::leaf",
        "dormant::inactive",
        "Wrapper::method",
        "read::inner",
        "inside::read::nested",
    ] {
        assert!(
            actual
                .records()
                .iter()
                .any(|record| record.context() == context),
            "missing {context}"
        );
    }
    assert!(
        !actual
            .records()
            .iter()
            .any(|record| record.context().contains("excluded"))
    );
    database
        .change_file(
            &library,
            source.replace("std::fs::read(\"x\")?;", "serde_json::from_str(\"x\")?;"),
        )
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_chains()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        changed
            .records()
            .iter()
            .any(|row| row.foreign_error_type().as_deref() == Some("serde_json::Error"))
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, source);
    database
        .change_file(&library, source.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_chains()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid inventory-only inputs fail closed"
    );
    shared::resync(database, root)?;
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(native.source_error_chains().into_diagnostic()?, actual);
    assert_eq!(
        projection::materialized_with(&native, root, hooks, "error_chain_rule_id")?,
        rows
    );
    Ok(())
}
