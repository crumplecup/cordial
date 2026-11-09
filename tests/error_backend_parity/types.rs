//! Type graph contracts preserve raw evidence, revisions and source-free replay.

use super::{projection, shared};
use cordial::SourceDatabase;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const HOOKS: &[&str] = &["error-type-inventory"];
const ATTR: &str = "internal_error_record_kind";
const CHILD: &str =
    "impl std::error::Error for CrossFile {}\nstruct UnrelatedChild { source: std::io::Error }\n";

pub const PACKAGE: &str = "type_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let child = root.join("src/child.rs");
    let source = include_str!("../fixtures/semantic_backend/error_types.rs.txt");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(&child, CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no types\n").into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let child = root.join("src/child.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_types().into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_types().into_diagnostic()?;
    assert_eq!(actual, expected, "complete raw evidence and snapshots");
    assert_eq!(
        actual.report("type_parity", true).into_diagnostic()?,
        expected.report("type_parity", true).into_diagnostic()?
    );
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    let report = actual.report("type_parity", true).into_diagnostic()?;
    let linked = report
        .nodes()
        .iter()
        .find(|node| node.type_path() == "Family::Io")
        .ok_or_else(|| miette::miette!("linked error variant"))?;
    assert!(linked.reaches_foreign());
    assert_eq!(linked.chain_depth(), 2);
    let paths: Vec<_> = report
        .nodes()
        .iter()
        .map(|node| node.type_path().clone())
        .collect();
    for path in [
        "Wrapper",
        "CrossFile",
        "Family::Text",
        "Family::Io",
        "dormant::Inactive",
        "dormant::inner::Nested",
        "CordialError",
        "Local",
        "Unicode",
    ] {
        assert!(paths.iter().any(|found| found == path), "missing {path}");
    }
    assert!(
        !paths
            .iter()
            .any(|path| path.contains("Hidden") || path == "Unrelated")
    );
    database
        .change_file(&child, "// implementor removed\n".into())
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_types()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        !changed
            .report("type_parity", true)
            .into_diagnostic()?
            .nodes()
            .iter()
            .any(|node| node.type_path() == "CrossFile")
    );
    assert_eq!(std::fs::read_to_string(&child).into_diagnostic()?, CHILD);
    database
        .change_file(&child, CHILD.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_types()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    shared::resync(database, root)?;
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        projection::materialized_with(&native, root, HOOKS, ATTR)?,
        rows
    );
    // Both shared materializers work without access to their source carrier.
    assert!(
        !projection::materialized_with(
            &native,
            root,
            &["error-site-inventory"],
            "error_site_kind"
        )?
        .is_empty()
    );
    Ok(())
}
