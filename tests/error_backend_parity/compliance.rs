//! Compliance contracts preserve complete evidence and source-free replay.

use super::{projection, shared};
use cordial::SourceDatabase;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const HOOKS: &[&str] = &["error-compliance-inventory"];
const ATTR: &str = "internal_error_record_kind";
const CHILD: &str =
    "fn loss() { builder.build().map_err(|cause| Local::new(cause.to_string())); }\n";

pub const PACKAGE: &str = "compliance_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let child = root.join("src/child.rs");
    let source = include_str!("../fixtures/semantic_backend/error_compliance.rs.txt");
    std::fs::write(&library, source).into_diagnostic()?;
    std::fs::write(&child, CHILD).into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no findings\n").into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let child = root.join("src/child.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_compliance().into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_compliance().into_diagnostic()?;
    for finding in actual.findings(false) {
        assert!(
            expected.findings(false).contains(&finding),
            "unexpected {finding:?}"
        );
    }
    for finding in expected.findings(false) {
        assert!(
            actual.findings(false).contains(&finding),
            "missing {finding:?}"
        );
    }
    assert_eq!(actual, expected, "complete records and snapshots");
    assert_eq!(actual.files().len(), 3);
    let findings = actual.findings(true);
    assert!(
        findings.len() > 30,
        "fixture must exercise each expression family"
    );
    for suffix in [
        "losses",
        "known_source",
        "dormant::loss",
        "included::loss",
        "child::loss",
    ] {
        assert!(
            findings.iter().any(|item| item.context().ends_with(suffix)),
            "missing {suffix}"
        );
    }
    assert!(
        !findings
            .iter()
            .any(|item| item.context().contains("excluded")
                || item.context().ends_with("preserving"))
    );
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    let macro_target = target
        .clone()
        .with_kinds(vec![cordial::CrateKind::ProcMacro]);
    let macro_baseline = projection::baseline(root, &macro_target)?
        .source_error_compliance()
        .into_diagnostic()?;
    let macro_native = database
        .project(&macro_target, &[])
        .into_diagnostic()?
        .source_error_compliance()
        .into_diagnostic()?;
    assert_eq!(macro_native, macro_baseline, "proc-macro terminus policy");
    assert!(macro_native.findings(false).len() < findings.len());
    database
        .change_file(&child, "// resolved\n".into())
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_compliance()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        !changed
            .findings(false)
            .iter()
            .any(|finding| finding.context() == "child::loss")
    );
    assert_eq!(std::fs::read_to_string(&child).into_diagnostic()?, CHILD);
    database
        .change_file(&child, CHILD.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_compliance()
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
    Ok(())
}
