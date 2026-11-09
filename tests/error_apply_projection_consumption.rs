//! Apply text comes from the loader's frozen bytes, never from later disk state.
#![cfg(feature = "internal_error_chain")]
#[path = "support/error_apply_source.rs"]
mod apply_source;
#[path = "support/source_projection.rs"]
mod projection;
use cordial::CrateTarget;
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn apply_text_uses_frozen_loader_bytes_and_rejects_a_fresh_mismatch() -> Result<()> {
    use cordial::{LoadContext, Loader, SessionBuilder, SourceLoadView, SourceLoader};
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    let path = root.join("src/lib.rs");
    std::fs::write(
        &path,
        r#"#[derive(Debug)]
struct Trouble { file: String }
impl std::error::Error for Trouble {}
impl Trouble { fn new() -> Self { Self { file: file!().to_owned() } } }
"#,
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("frozen_apply", root);
    let graph = projection::baseline(root, &target)?;
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let expected = apply_source::materialized_with_source(graph.clone(), source, &session)?;
    assert!(
        expected["units"]
            .as_array()
            .is_some_and(|units| !units.is_empty())
    );
    // An unrelated file, even malformed Rust, must not become another planner
    // input. The IR defines the complete file universe for this snapshot.
    std::fs::write(root.join("src/later.rs"), "fn malformed(").into_diagnostic()?;
    std::fs::write(&path, "fn changed() {}").into_diagnostic()?;
    assert_eq!(
        expected,
        apply_source::materialized_with_source(graph.clone(), source, &session)?
    );
    let fresh = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = fresh
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("fresh source view"))?;
    assert!(apply_source::materialized_with_source(graph, source, &session).is_err());
    Ok(())
}
