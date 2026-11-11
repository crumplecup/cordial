//! Getters: trivial and clone getters, copy getters, and reference and boxed returns.

use cordial::{
    DERIVES_ETIQUETTE, DeriveRuleId, DerivesThresholds, PathInclusionFacts, RunAll, Session,
    SessionBuilder, scan_derives_rust_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/derives_scan.rs"]
mod derives_scan;

use derives_scan::scan_findings;

const TRIVIAL_GETTER: &str = r#"struct Widget {
    name: String,
    count: u32,
}

impl Widget {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}
"#;

#[test]
fn derives_etiquette_detects_trivial_getters() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), TRIVIAL_GETTER)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*DERIVES_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("derives.csv"))
        .into_diagnostic()
        .wrap_err("derives csv")?;
    assert!(csv.contains("DERIVE-GETTER-001"));
    assert!(csv.contains("Widget::name"));
    assert!(csv.contains("Widget::count"));

    let checklist = fs::read_to_string(findings_dir.join("derives.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("derive_getters"));

    let summary = fs::read_to_string(findings_dir.join("derives-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("getter **2**"));
    Ok(())
}

#[test]
fn scan_derives_rust_source_flags_trivial_getters() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("trivial_getter.rs");
    fs::write(&file, TRIVIAL_GETTER)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_derives_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        DerivesThresholds::default(),
        &PathInclusionFacts::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings
            .iter()
            .filter(|record| record.rule_id() == DeriveRuleId::Getter001)
            .count(),
        2
    );
    Ok(())
}

#[test]
fn clone_getter_on_a_non_copy_field_has_no_derive_getters_equivalent() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    name: String,
}

impl Widget {
    pub fn name(&self) -> String {
        self.name.clone()
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    assert!(
        findings
            .iter()
            .all(|record| record.rule_id() != DeriveRuleId::Getter001),
        "`self.field.clone()` proves nothing about Copy, and derive_getters \
         has no action that returns an owned clone of a non-Copy field \
         (only skip/rename/copy exist, confirmed against its own source) \
         -- must not recommend #[getter(copy)], which would fail to \
         compile for a String field: {findings:?}"
    );
    Ok(())
}

#[test]
fn bare_field_return_recommends_copy_getters() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    id: u32,
}

impl Widget {
    pub fn id(&self) -> u32 {
        self.id
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let getter = findings
        .iter()
        .find(|record| record.rule_id() == DeriveRuleId::Getter001)
        .ok_or_else(|| miette::miette!("bare self.field return should flag DERIVE-GETTER-001"))?;
    assert!(
        getter.recommendation().contains("getter(copy)"),
        "returning the field by value (not `&self.field`) only compiles \
         because it's Copy, and a plain #[derive(Getters)] would return a \
         reference instead -- must steer to #[getter(copy)]: {}",
        getter.recommendation()
    );
    Ok(())
}

#[test]
fn reference_field_return_does_not_recommend_copy_getters() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    id: u32,
}

impl Widget {
    pub fn id(&self) -> &u32 {
        &self.id
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let getter = findings
        .iter()
        .find(|record| record.rule_id() == DeriveRuleId::Getter001)
        .ok_or_else(|| miette::miette!("&self.field return should flag DERIVE-GETTER-001"))?;
    assert!(
        !getter.recommendation().contains("getter(copy)"),
        "returning `&self.field` is exactly what a plain #[derive(Getters)] \
         already produces -- must NOT steer to #[getter(copy)], which would \
         change the return type from &u32 to u32: {}",
        getter.recommendation()
    );
    Ok(())
}

#[test]
fn boxed_inner_reference_getter_has_no_derive_getters_equivalent() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    kind: Box<WidgetKind>,
}

enum WidgetKind {
    Basic,
}

impl Widget {
    pub fn kind(&self) -> &WidgetKind {
        &self.kind
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    assert!(
        findings
            .iter()
            .all(|record| record.rule_id() != DeriveRuleId::Getter001),
        "`&self.boxed_field` returning `&Inner` relies on deref coercion; \
         derive_getters would return `&Box<Inner>` instead: {findings:?}"
    );
    Ok(())
}

#[test]
fn boxed_field_reference_getter_still_flags_when_returning_box() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    kind: Box<WidgetKind>,
}

enum WidgetKind {
    Basic,
}

impl Widget {
    pub fn kind(&self) -> &Box<WidgetKind> {
        &self.kind
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    assert_eq!(
        findings
            .iter()
            .filter(|record| record.rule_id() == DeriveRuleId::Getter001)
            .count(),
        1,
        "`&self.boxed_field` returning `&Box<Inner>` is exactly the shape \
         derive_getters can generate: {findings:?}"
    );
    Ok(())
}
