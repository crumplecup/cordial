use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CFG_SCATTER_ETIQUETTE, CfgScatterThresholds, CfgSiteKind, RunAll, Session, SessionBuilder,
    scan_cfg_scatter_rust_source,
};

fn test_thresholds() -> CfgScatterThresholds {
    CfgScatterThresholds::new(2, 4)
}

const SCATTERED_SOURCE: &str = r#"
#[cfg(feature = "widgets")]
use std::collections::HashMap;

#[cfg(feature = "widgets")]
fn build_widget() -> u32 {
    1
}

#[cfg(feature = "widgets")]
struct Widget {
    id: u32,
}

#[cfg(feature = "widgets")]
impl Widget {
    fn new() -> Self {
        Self { id: 0 }
    }
}

struct Gadget {
    #[cfg(feature = "gizmo")]
    gizmo_id: u32,
    #[cfg(feature = "gizmo")]
    gizmo_name: String,
    label: String,
}
"#;

#[test]
fn scan_cfg_scatter_rust_source_flags_scattered_predicate_not_fields() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, SCATTERED_SOURCE)
        .into_diagnostic()
        .wrap_err("write source")?;

    let groups =
        scan_cfg_scatter_rust_source(SCATTERED_SOURCE, &file, fixture.path(), fixture.path())
            .into_diagnostic()
            .wrap_err("scan")?;

    let widgets = groups
        .iter()
        .find(|group| group.predicate.contains("widgets"))
        .ok_or_else(|| miette::miette!("widgets predicate group present"))?;
    assert!(
        widgets.is_scatter(&test_thresholds()),
        "fn+struct+impl+use sharing one predicate should flag as scatter"
    );

    let gizmo_flagged = groups
        .iter()
        .find(|group| group.predicate.contains("gizmo"))
        .map(|group| group.is_scatter(&test_thresholds()))
        .unwrap_or(false);
    assert!(
        !gizmo_flagged,
        "field-only cfg gating on one struct must never be flagged as scatter"
    );
    Ok(())
}

#[test]
fn cfg_scatter_etiquette_detects_scattered_predicate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), SCATTERED_SOURCE)
        .into_diagnostic()
        .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CFG_SCATTER_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule().id() == "CFG-SCATTER-001"),
        "expected a scattered-cfg finding for the `widgets` predicate"
    );

    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("cfg-scatter.csv").is_file());
    assert!(findings_dir.join("cfg-scatter.checklist.md").is_file());
    assert!(findings_dir.join("cfg-scatter-summary.md").is_file());
    Ok(())
}

const TRAIT_SCATTERED_SOURCE: &str = r#"
#[cfg(feature = "reporting")]
use std::fmt::Debug;

trait Reporter {
    #[cfg(feature = "reporting")]
    fn report(&self) -> String {
        String::new()
    }

    #[cfg(feature = "reporting")]
    const LABEL: &'static str = "report";

    #[cfg(feature = "reporting")]
    type Output;
}

struct Impl;

impl Reporter for Impl {
    #[cfg(feature = "reporting")]
    type Output = String;
}
"#;

#[test]
fn scan_cfg_scatter_rust_source_flags_trait_default_methods() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, TRAIT_SCATTERED_SOURCE)
        .into_diagnostic()
        .wrap_err("write source")?;

    let groups = scan_cfg_scatter_rust_source(
        TRAIT_SCATTERED_SOURCE,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let reporting = groups
        .iter()
        .find(|group| group.predicate.contains("reporting"))
        .ok_or_else(|| miette::miette!("reporting predicate group present"))?;

    let kinds: Vec<_> = reporting.occurrences.iter().map(|o| o.kind).collect();
    assert!(
        kinds.contains(&CfgSiteKind::TraitFn),
        "cfg on a trait default method must be visible to the scanner, got {kinds:?}"
    );
    assert!(
        reporting.is_scatter(&test_thresholds()),
        "use+trait_fn+const+type_alias sharing one predicate should flag as scatter"
    );
    Ok(())
}

#[test]
fn cfg_scatter_default_thresholds() {
    cordial::init_tracing();
    let thresholds = CfgScatterThresholds::default();
    assert_eq!(thresholds.min_distinct_kinds(), 2);
    assert_eq!(thresholds.min_occurrences(), 5);
}
