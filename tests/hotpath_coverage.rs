//! Coverage policy consumes the shared loader/IR, including untraced declarations.
#![cfg(feature = "tracing")]
use cordial::SourceDatabase;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn canonical_inventory_includes_untraced_functions_and_escaping_bodies() -> Result {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("src"))?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='coverage_fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    let source = root.path().join("src/lib.rs");
    std::fs::write(
        &source,
        r#"
        #![no_std]
        pub fn untraced() { let _escaped = || 42u8; }
        #[hotpath::measure]
        pub async fn measured() { let _escaped = async { 23u8 }; }
        #[cfg_attr(feature = "unknown", hotpath::measure)]
        pub fn conditional() {}
    "#,
    )?;
    let mut database = SourceDatabase::load_with(
        root.path(),
        &cordial::SemanticCargoOptions::default()
            .with_cfg_planning(cordial::CfgPlanning::Explicit)
            .with_skip_file_scan_priming(true),
    )?;
    let ir = database.project(
        &cordial::CrateTarget::new("coverage_fixture", root.path()),
        &[source],
    )?;
    let rows = cordial::profiling::operation_coverage(&ir)?;
    let facts = |name: &str| {
        rows.iter()
            .find(|row| row.path.rsplit("::").next() == Some(name))
            .and_then(|row| row.facts.as_ref())
            .ok_or("missing function facts")
    };
    assert!(!facts("untraced")?.measured);
    assert_eq!(facts("untraced")?.closures, 1);
    assert!(facts("measured")?.measured);
    assert!(facts("measured")?.asynchronous);
    assert_eq!(facts("measured")?.async_blocks, 1);
    assert!(facts("conditional")?.conditional);
    assert!(!facts("conditional")?.measured);
    Ok(())
}

#[test]
fn every_inventory_region_has_visible_evidence_and_limits() {
    let boundaries = cordial::profiling::boundaries();
    assert_eq!(boundaries.len(), 10);
    assert!(
        boundaries
            .iter()
            .all(|row| !row.evidence.is_empty() && !row.unverified.is_empty())
    );
}
