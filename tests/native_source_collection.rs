//! Production syntax inventories use the canonical database without Cargo.
#![cfg(all(feature = "tracing", feature = "error_sites"))]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use cordial::{
    CrateIr, CrateTarget, IrView, LoadContext, Loader, SessionBuilder, SourceLoadView,
    SourceLoader, TracingStdioPolicy,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

#[derive(Clone, Default)]
struct Parses {
    native: Arc<AtomicUsize>,
    compatibility: Arc<AtomicUsize>,
}

impl<S: tracing::Subscriber> Layer<S> for Parses {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _: &tracing::Id,
        _: Context<'_, S>,
    ) {
        let metadata = attributes.metadata();
        if metadata.target() == "ra_ap_base_db::editioned_file_id" && metadata.name() == "parse" {
            self.native.fetch_add(1, Ordering::Relaxed);
        }
        if metadata.target() == "cordial::loader::source_work" && metadata.name() == "parse" {
            self.compatibility.fetch_add(1, Ordering::Relaxed);
        }
    }
}

const SOURCE: &str =
    "#[cfg(any())]\nmod dormant { pub fn run() { source()?; println!(\"hello\"); } }\n";

#[test]
fn supplied_sources_need_neither_a_manifest_nor_a_file() -> Result<()> {
    cordial::init_cli_tracing();
    let access = Parses::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(access.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let tree = fixture.path().join("src");
    let file = tree.join("missing.rs");
    let sites = cordial::scan_error_sites_rust_source(SOURCE, &file, &tree, fixture.path())
        .into_diagnostic()?;
    let prints = cordial::scan_tracing_print_rust_source(
        SOURCE,
        &file,
        &tree,
        fixture.path(),
        &TracingStdioPolicy::default(),
    )
    .into_diagnostic()?;
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].context(), "missing::dormant::run");
    assert_eq!(sites[0].file(), std::path::Path::new("src/missing.rs"));
    assert_eq!(sites[0].line(), 2);
    assert_eq!(sites[0].source_snippet(), "source(…)");
    assert_eq!(prints.len(), 1);
    assert_eq!(prints[0].context(), "missing::dormant");
    assert_eq!(prints[0].line(), 2);
    assert!(!file.exists());
    assert_eq!(
        access.native.load(Ordering::Relaxed),
        2,
        "one parse per standalone analysis"
    );
    assert_eq!(access.compatibility.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn malformed_supplied_source_preserves_the_native_error_chain() -> Result<()> {
    cordial::init_cli_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let file = fixture.path().join("missing.rs");
    let error = cordial::scan_error_sites_rust_source(
        "fn invalid( {",
        &file,
        fixture.path(),
        fixture.path(),
    )
    .err()
    .ok_or_else(|| miette::miette!("invalid source was accepted"))?;
    let cordial::CordialErrorKind::RustParse(source) = error.kind() else {
        return Err(miette::miette!("lost native diagnostic: {error}"));
    };
    assert_eq!(source.path(), &file.display().to_string());
    let cause =
        std::error::Error::source(source).ok_or_else(|| miette::miette!("missing syntax error"))?;
    assert!(cause.is::<ra_ap_syntax::SyntaxError>());
    Ok(())
}

#[test]
fn manifest_free_scans_use_native_inventories() -> Result<()> {
    cordial::init_cli_tracing();
    let access = Parses::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(access.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    std::fs::write(fixture.path().join("src/lib.rs"), SOURCE).into_diagnostic()?;
    let sites = cordial::scan_crate_error_sites(fixture.path()).into_diagnostic()?;
    let prints = cordial::scan_crate_tracing_print(fixture.path(), &TracingStdioPolicy::default())
        .into_diagnostic()?;
    assert_eq!(sites.len(), 1);
    assert_eq!(prints.len(), 1);
    assert_eq!(access.native.load(Ordering::Relaxed), 2);
    assert_eq!(access.compatibility.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn loader_inventories_share_a_parse_and_keep_their_frozen_input() -> Result<()> {
    cordial::init_cli_tracing();
    let access = Parses::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(access.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    let file = fixture.path().join("src/lib.rs");
    std::fs::write(&file, SOURCE).into_diagnostic()?;
    let target = CrateTarget::new("fixture", fixture.path());
    let session = SessionBuilder::new(fixture.path()).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut first = CrateIr::new("fixture");
    source.populate_ir(&mut first).into_diagnostic()?;
    assert_eq!(
        access.native.load(Ordering::Relaxed),
        1,
        "both inventories must share the upstream query"
    );
    std::fs::write(&file, "pub fn changed() {}\n").into_diagnostic()?;
    let mut repeated = CrateIr::new("fixture");
    source.populate_ir(&mut repeated).into_diagnostic()?;
    assert_eq!(access.native.load(Ordering::Relaxed), 1);
    assert_eq!(
        first.source_error_sites().into_diagnostic()?,
        repeated.source_error_sites().into_diagnostic()?
    );
    assert_eq!(
        first.source_observability().into_diagnostic()?,
        repeated.source_observability().into_diagnostic()?
    );
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    assert_eq!(
        first.source_policy_inventory().into_diagnostic()?,
        repeated.source_policy_inventory().into_diagnostic()?
    );
    #[cfg(feature = "antipatterns")]
    assert_eq!(
        first.source_antipatterns().into_diagnostic()?,
        repeated.source_antipatterns().into_diagnostic()?
    );
    let fresh = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = fresh
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut changed = CrateIr::new("fixture");
    source.populate_ir(&mut changed).into_diagnostic()?;
    assert_ne!(
        first.source_error_sites().into_diagnostic()?,
        changed.source_error_sites().into_diagnostic()?
    );
    assert_ne!(
        first.source_observability().into_diagnostic()?,
        changed.source_observability().into_diagnostic()?
    );
    assert_eq!(access.native.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
#[cfg(all(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives"
))]
fn standalone_policy_fixtures_use_native_syntax_without_disk_inputs() -> Result<()> {
    cordial::init_cli_tracing();
    let access = Parses::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(access.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let tree = fixture.path().join("src");
    let file = tree.join("lib.rs");
    let source = r#"#![allow(unused)]
#[cfg(feature = "inactive")]
mod dormant {
    use other::*;
    pub struct Value { pub value: u8 }
    #[cfg(feature = "inactive")] fn work() {}
    #[cfg(test)] mod tests { #[test] fn works() {} }
}
"#;
    assert!(
        !cordial::scan_allows_rust_source(source, &file, &tree, fixture.path())
            .into_diagnostic()?
            .is_empty()
    );
    assert!(
        !cordial::scan_cfg_hygiene_rust_source(source, &file, &tree, fixture.path())
            .into_diagnostic()?
            .is_empty()
    );
    assert!(
        !cordial::scan_cfg_scatter_rust_source(source, &file, &tree, fixture.path())
            .into_diagnostic()?
            .is_empty()
    );
    assert!(
        !cordial::scan_glob_imports_rust_source(source, &file, &tree, fixture.path())
            .into_diagnostic()?
            .is_empty()
    );
    assert!(
        !cordial::scan_inline_tests_rust_source(source, &file, &tree, fixture.path())
            .into_diagnostic()?
            .is_empty()
    );
    assert!(
        !cordial::scan_modularity_rust_source(
            source,
            &file,
            &tree,
            fixture.path(),
            &cordial::ModularityThresholds::default(),
        )
        .into_diagnostic()?
        .is_empty()
    );
    assert!(
        !cordial::scan_pageantry_rust_source(
            source,
            &file,
            &tree,
            fixture.path(),
            &cordial::PageantryThresholds::default(),
        )
        .into_diagnostic()?
        .is_empty()
    );
    cordial::scan_derives_rust_source(
        source,
        &file,
        &tree,
        fixture.path(),
        cordial::DerivesThresholds::default(),
        &cordial::PathInclusionFacts::default(),
    )
    .into_diagnostic()?;
    assert!(!file.exists());
    assert_eq!(access.native.load(Ordering::Relaxed), 8);
    assert_eq!(access.compatibility.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
#[cfg(feature = "antipatterns")]
fn antipattern_and_contract_fixtures_use_only_native_inputs() -> Result<()> {
    cordial::init_cli_tracing();
    let access = Parses::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(access.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let tree = fixture.path().join("src");
    let file = tree.join("missing.rs");
    let sites = cordial::scan_antipatterns_rust_source(
        "fn run(_unused: u8) -> Result<(), String> { Ok(()) }",
        &file,
        &tree,
        fixture.path(),
    )
    .into_diagnostic()?;
    assert_eq!(sites.len(), 2);
    assert!(sites.iter().all(|site| site.context() == "missing::run"
        && site.file() == std::path::Path::new("src/missing.rs")
        && site.line() == 1));
    let creusot = cordial::scan_creusot_contract_bounds_source(
        "#[requires(x > 0)] fn run(x: i32) {}",
        &file,
        &tree,
        &[],
    )
    .into_diagnostic()?;
    let kani = cordial::scan_kani_contract_bounds_source(
        "#[kani::proof] fn run() { assert!(x > 0); }",
        &file,
        &tree,
        &[],
    )
    .into_diagnostic()?;
    let verus = cordial::scan_verus_contract_bounds_source(
        "verus! { fn run(x: int) requires x > 0, {} }",
        &file,
        &tree,
        &[],
    )
    .into_diagnostic()?;
    for sites in [creusot, kani, verus] {
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].file(), &file);
        assert_eq!(sites[0].line(), 1);
    }
    assert!(!file.exists());
    assert_eq!(access.native.load(Ordering::Relaxed), 4);
    assert_eq!(access.compatibility.load(Ordering::Relaxed), 0);
    Ok(())
}
