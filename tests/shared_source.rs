use std::fs;

use cordial::{CrateTarget, LoadContext, Loader, SessionBuilder, SourceLoadView, SourceLoader};
use miette::IntoDiagnostic;

#[test]
fn source_view_reuses_the_same_snapshot_and_tree() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let src_root = fixture.path().join("src");
    fs::create_dir_all(&src_root).into_diagnostic()?;
    let path = src_root.join("lib.rs");
    fs::write(&path, "fn original() {}").into_diagnostic()?;

    let session = SessionBuilder::new(fixture.path()).build();
    let target = CrateTarget::new("fixture", fixture.path());
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let view = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source loader returned another view"))?;

    let first = view.parsed_file(&path, "first").into_diagnostic()?;
    assert_eq!(first.path(), path);
    assert_eq!(first.source(), "fn original() {}");
    assert_eq!(first.syntax().items.len(), 1);

    fs::write(src_root.join("skip.rs"), "fn excluded() {}").into_diagnostic()?;
    let selected = view
        .parsed_rust_files_under(&src_root, "walk", |candidate| candidate == path)
        .into_diagnostic()?;
    assert_eq!(selected.len(), 1);
    assert!(std::ptr::eq(first.syntax(), selected[0].syntax()));

    fs::write(&path, "fn changed() {}").into_diagnostic()?;
    let second = view.parsed_file(&path, "second").into_diagnostic()?;
    assert!(std::ptr::eq(first.syntax(), second.syntax()));
    assert_eq!(second.source(), "fn original() {}");

    let reloaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let fresh_view = reloaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source loader returned another view"))?;
    let fresh = fresh_view.parsed_file(&path, "fresh").into_diagnostic()?;
    assert_eq!(fresh.source(), "fn changed() {}");
    assert!(!std::ptr::eq(first.syntax(), fresh.syntax()));
    Ok(())
}
