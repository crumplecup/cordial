//! Canonical architecture facts preserve library scope and virtual edits.
#![cfg(feature = "internal_error_chain")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]
#[path = "support/error_sites_default.rs"]
mod default_sites;
#[path = "support/error_sites_projection.rs"]
mod projection;
use cordial::{CrateTarget, ErrorFieldStrategy, IrView, SourceErrorArchitecture};
use miette::{IntoDiagnostic, Result};
const HOOKS: &[&str] = &["error-architecture-inventory"];
const ATTR: &str = "internal_error_record_kind";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn canonical_architecture_matches_complete_contracts_and_revisions() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    for dir in ["src/inline", "src/nested"] {
        std::fs::create_dir_all(root.join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='architecture_parity'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy='borrowed'\n",
    )
    .into_diagnostic()?;
    let library = root.join("src/lib.rs");
    let source = format!(
        "mod empty; mod nested; mod inline {{ mod leaf; }} #[path = \"renamed.rs\"] mod linked; #[cfg(test)] mod test_only;\n{}\n{}",
        include_str!("fixtures/semantic_backend/error_architecture.rs.txt"),
        include_str!("fixtures/semantic_backend/error_architecture_edges.rs.txt").replace(
            "//! Architecture syntax policy remains independent of active Cargo cfgs.",
            "// Architecture edges."
        )
    );
    std::fs::write(&library, &source).into_diagnostic()?;
    for (path, text) in [
        ("empty.rs", "// empty, reachable file\n"),
        ("nested/mod.rs", "mod leaf;\n"),
        (
            "nested/leaf.rs",
            "#[derive(Error)] struct NestedSource { file: String }\n",
        ),
        (
            "inline/leaf.rs",
            "#[derive(Error)] struct InlineSource { file: String }\n",
        ),
        (
            "renamed.rs",
            "fn cross_file(message: String) -> ParentError { ParentError::new(ParentKind::Native(NativeSource::new(message))) }\nfn use_it() { cross_file(\"cross-file\"); }\n",
        ),
        (
            "test_only.rs",
            "#[derive(Error)] struct IgnoredModuleSource { message: String }\n",
        ),
        ("main.rs", "mod binary; fn main() {}\n"),
        (
            "binary.rs",
            "#[derive(Error)] struct BinarySource { message: String }\n",
        ),
        (
            "orphan.rs",
            "#[derive(Error)] struct OrphanSource { message: String }\n",
        ),
    ] {
        std::fs::write(root.join("src").join(path), text).into_diagnostic()?;
    }
    let target = CrateTarget::new("architecture_parity", root);
    let baseline = projection::baseline(root, &target)?;
    let expected = baseline.source_error_architecture().into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_error_architecture().into_diagnostic()?;
    compare(&actual, &expected)?;
    assert_eq!(actual.files().len(), 6);
    for strategy in [ErrorFieldStrategy::Owned, ErrorFieldStrategy::Borrowed] {
        for threshold in [0, 1, 100] {
            assert_eq!(
                actual
                    .assess(threshold, strategy, false)
                    .into_diagnostic()?,
                expected
                    .assess(threshold, strategy, false)
                    .into_diagnostic()?
            );
        }
    }
    let assessment = actual
        .assess(0, ErrorFieldStrategy::Borrowed, false)
        .into_diagnostic()?;
    assert!(assessment.len() > 10);
    let rows = projection::materialized_with(&native, root, HOOKS, ATTR)?;
    assert_eq!(
        rows,
        projection::materialized_with(&baseline, root, HOOKS, ATTR)?
    );
    database
        .change_file(
            &library,
            source
                .replace("message: String,", "message: &'static str,")
                .replace("#[track_caller]", "#[doc = \"no track caller\"]"),
        )
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_architecture()
        .into_diagnostic()?;
    assert_ne!(actual, changed);
    assert_ne!(
        assessment,
        changed
            .assess(0, ErrorFieldStrategy::Borrowed, false)
            .into_diagnostic()?
    );
    assert_eq!(std::fs::read_to_string(&library).into_diagnostic()?, source);
    database
        .change_file(&library, source.clone())
        .into_diagnostic()?;
    compare(
        &database
            .project(&target, &[])
            .into_diagnostic()?
            .source_error_architecture()
            .into_diagnostic()?,
        &actual,
    )?;
    database
        .change_file(&library, source.replace("mod nested;", ""))
        .into_diagnostic()?;
    let unlinked = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_error_architecture()
        .into_diagnostic()?;
    assert_eq!(
        unlinked.files().len(),
        4,
        "module selection follows virtual syntax"
    );
    database.change_file(&library, source).into_diagnostic()?;
    database
        .change_file(&root.join("src/empty.rs"), "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[]).is_err());
    drop(database);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert!(!default_sites::materialized(&native, root)?.is_empty());
    assert_eq!(
        actual
            .assess(0, ErrorFieldStrategy::Borrowed, false)
            .into_diagnostic()?,
        assessment
    );
    assert_eq!(
        projection::materialized_with(&native, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}
#[tracing::instrument(level = "debug", skip(actual, expected), err(level = "warn"))]
fn compare(actual: &SourceErrorArchitecture, expected: &SourceErrorArchitecture) -> Result<()> {
    let actual = serde_json::to_value(actual).into_diagnostic()?;
    let expected = serde_json::to_value(expected).into_diagnostic()?;
    for group in [
        "structs",
        "enums",
        "constructors",
        "map_err_sites",
        "fanout_wrappers",
        "fanout_calls",
        "error_impls",
        "fields",
        "copy_impls",
    ] {
        if let (Some(a), Some(e)) = (
            actual["catalog"][group].as_array(),
            expected["catalog"][group].as_array(),
        ) {
            assert_eq!(a.len(), e.len(), "{group} length");
            for (index, (a, e)) in a.iter().zip(e).enumerate() {
                assert_eq!(a, e, "{group} record {index}");
            }
        } else {
            assert_eq!(
                actual["catalog"][group], expected["catalog"][group],
                "{group}"
            );
        }
    }
    assert_eq!(
        actual, expected,
        "complete inventory including source snapshots"
    );
    Ok(())
}
