//! Error-site payload parity, virtual revisions and source-independent replay.

use super::{default_sites, projection, shared};
use cordial::{
    ATTR_SOURCE_ERROR_SITES, CrateIr, CrateTarget, ErrorSiteKind, IrView, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const LIB: &str = include_str!("../fixtures/semantic_backend/error_sites.rs.txt");

pub const PACKAGE: &str = "error_sites_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    for dir in ["src", "tests/fixtures", "tests/parity", "examples"] {
        std::fs::create_dir_all(root.join(dir)).into_diagnostic()?;
    }
    let library = root.join("src/lib.rs");
    std::fs::write(&library, LIB).into_diagnostic()?;
    std::fs::write(root.join("src/child.rs"), "pub fn leaf() { source()?; }\n")
        .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// complete empty inventory\n").into_diagnostic()?;
    std::fs::write(
        root.join("tests/check.rs"),
        "#[test]\nfn check() { integration().map_err(convert)?; }\n",
    )
    .into_diagnostic()?;
    for path in [
        "tests/fixtures/invalid.rs",
        "tests/parity/invalid.rs",
        "examples/ignored.rs",
    ] {
        std::fs::write(root.join(path), "fn invalid( {").into_diagnostic()?;
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let library = root.join("src/lib.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let legacy = projection::baseline(root, &target)?;
    let expected = legacy.source_error_sites().into_diagnostic()?;
    assert_eq!(expected.files().len(), 4);
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_sites().into_diagnostic()?;
    assert_eq!(
        actual, expected,
        "complete records, source digests and file coverage"
    );
    assert_eq!(
        actual.records(),
        cordial::scan_crate_error_sites(root).into_diagnostic()?
    );
    for kind in [
        ErrorSiteKind::QuestionMark,
        ErrorSiteKind::MapErr,
        ErrorSiteKind::ReturnErr,
        ErrorSiteKind::IfLetErr,
        ErrorSiteKind::MatchErr,
        ErrorSiteKind::OkOr,
    ] {
        assert!(
            actual.records().iter().any(|row| row.kind() == kind),
            "missing {kind}"
        );
    }
    for context in [
        "child::leaf",
        "dormant::probe",
        "pipeline::nested",
        "Holder::method",
        "nested_default",
        "inline::nested::outer::leaf",
        "included::check",
        "check::check",
    ] {
        assert!(
            actual.records().iter().any(|row| row.context() == context),
            "missing context {context}"
        );
    }
    assert!(
        !actual
            .records()
            .iter()
            .any(|row| row.context().contains("excluded"))
    );
    assert_eq!(
        default_sites::materialized(&legacy, root)?,
        default_sites::materialized(&native, root)?,
        "all materialized node attributes and spans"
    );
    let mut corrupt = native.clone();
    let payload = serde_json::to_value(&actual).into_diagnostic()?;
    for key in [
        "kind",
        "context",
        "file",
        "line",
        "source_snippet",
        "site_snippet",
    ] {
        let mut broken = payload.clone();
        let index = actual
            .files()
            .iter()
            .position(|file| !file.sites().is_empty())
            .ok_or_else(|| miette::miette!("site evidence"))?;
        broken["files"][index]["sites"][0]
            .as_object_mut()
            .ok_or_else(|| miette::miette!("site payload"))?
            .remove(key);
        corrupt
            .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_SITES, broken)
            .into_diagnostic()?;
        assert!(corrupt.source_error_sites().is_err(), "required {key}");
    }
    assert!(CrateIr::new("empty").source_error_sites().is_err());
    database
        .change_file(
            &library,
            LIB.replace("input().map_err(Error::from)?", "changed().ok_or(Error)?"),
        )
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_sites()
        .into_diagnostic()?;
    assert_ne!(changed, actual);
    assert!(
        changed
            .records()
            .iter()
            .any(|row| row.source_snippet() == "changed(…)" && row.kind() == ErrorSiteKind::OkOr)
    );
    assert!(
        !changed
            .records()
            .iter()
            .any(|row| row.source_snippet() == "input(…)")
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, LIB);
    database
        .change_file(&library, LIB.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_sites()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&library, "fn broken( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "syntax recovery cannot establish a complete census"
    );
    shared::resync(database, root)?;
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::remove_dir_all(root.join("tests")).into_diagnostic()?;
    assert_eq!(
        native.source_error_sites().into_diagnostic()?.records(),
        expected.records()
    );
    assert_eq!(
        default_sites::materialized(&native, root)?,
        default_sites::materialized(&legacy, root)?
    );
    Ok(())
}
