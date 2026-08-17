use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    DERIVES_ETIQUETTE, DeriveRuleId, RunAll, Session, SessionBuilder, scan_derives_rust_source,
};

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
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), TRIVIAL_GETTER)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&DERIVES_ETIQUETTE)
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
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings
            .iter()
            .filter(|record| record.rule_id == DeriveRuleId::Getter001)
            .count(),
        2
    );
    Ok(())
}
