use std::fs;

use cordial::{
    CrateTarget, LoadContext, Loader, SessionBuilder, SharedMacroSyntax, SourceLoadView,
    SourceLoader,
};
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
    assert!(first.matches_disk().into_diagnostic()?);

    fs::write(src_root.join("skip.rs"), "fn excluded() {}").into_diagnostic()?;
    let selected = view
        .parsed_rust_files_under(&src_root, "walk", |candidate| candidate == path)
        .into_diagnostic()?;
    assert_eq!(selected.len(), 1);
    assert!(std::ptr::eq(first.syntax(), selected[0].syntax()));

    fs::write(&path, "fn changed() {}").into_diagnostic()?;
    assert!(!first.matches_disk().into_diagnostic()?);
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
    assert!(fresh.matches_disk().into_diagnostic()?);
    assert_ne!(first.source_digest(), fresh.source_digest());
    assert!(!std::ptr::eq(first.syntax(), fresh.syntax()));
    Ok(())
}

#[test]
fn macro_rust_islands_share_the_source_snapshot() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    let path = fixture.path().join("src/lib.rs");
    let text = "amenable_derive::gallery_harness! { kani, SOURCE, { fn proof() { assert!(true); } } }\nunknown!(arbitrary tokens);";
    fs::write(&path, text).into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path()).build();
    let target = CrateTarget::new("fixture", fixture.path());
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let view = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    let first = view.parsed_file(&path, "first").into_diagnostic()?;
    let syn::Item::Macro(invocation) = &first.syntax().items[0] else {
        return Err(miette::miette!("missing macro"));
    };
    let payload = first
        .macro_syntax(&invocation.mac)
        .ok_or_else(|| miette::miette!("missing payload"))?;
    let SharedMacroSyntax::Items(items) = payload else {
        return Err(miette::miette!("expected items"));
    };
    let syn::Item::Fn(function) = &items[0] else {
        return Err(miette::miette!("expected function"));
    };
    let syn::Stmt::Macro(assertion) = &function.block.stmts[0] else {
        return Err(miette::miette!("expected assertion"));
    };
    assert!(
        matches!(first.macro_syntax(&assertion.mac), Some(SharedMacroSyntax::Expressions(expressions)) if expressions.len() == 1)
    );
    let second = view.parsed_file(&path, "second").into_diagnostic()?;
    assert!(std::ptr::eq(
        payload,
        second
            .macro_syntax(&invocation.mac)
            .ok_or_else(|| miette::miette!("missing reused payload"))?
    ));
    let syn::Item::Macro(unknown) = &first.syntax().items[1] else {
        return Err(miette::miette!("missing unknown macro"));
    };
    assert!(first.macro_syntax(&unknown.mac).is_none());
    fs::write(&path, "unknown!(changed);").into_diagnostic()?;
    assert!(!first.matches_disk().into_diagnostic()?);
    assert!(std::ptr::eq(
        payload,
        first
            .macro_syntax(&invocation.mac)
            .ok_or_else(|| miette::miette!("lost snapshot payload"))?
    ));
    Ok(())
}
