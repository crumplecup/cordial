//! Function-length findings: oversize bodies, methods and trait defaults, and what the scan ignores.

use cordial::{
    MODULARITY_ETIQUETTE, ModularityKind, ModularityThresholds, RunAll, Session, SessionBuilder,
    scan_modularity_rust_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/modularity_thresholds.rs"]
mod modularity_thresholds;

#[path = "support/modularity_function_fixtures.rs"]
mod modularity_function_fixtures;

use modularity_function_fixtures::large_function_fixture;
use modularity_thresholds::test_thresholds;

const HANDLERS_RS: &str = include_str!("fixtures/quality/modularity/handlers.rs");

#[test]
fn modularity_etiquette_detects_large_functions() -> miette::Result<()> {
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
    fs::write(fixture.path().join("src/lib.rs"), large_function_fixture())
        .into_diagnostic()
        .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule().id() == "MODULARITY-FUNCTION"),
        "expected function findings with default thresholds"
    );

    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("modularity.csv").is_file());
    assert!(findings_dir.join("modularity.checklist.md").is_file());
    assert!(findings_dir.join("modularity-summary.md").is_file());
    Ok(())
}

#[test]
fn scan_modularity_rust_source_ranks_handlers() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("handlers.rs");
    fs::write(&file, HANDLERS_RS)
        .into_diagnostic()
        .wrap_err("write handlers")?;

    let findings = scan_modularity_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        &test_thresholds(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let functions: Vec<_> = findings
        .iter()
        .filter(|record| record.kind() == ModularityKind::Function)
        .collect();
    assert!(
        functions
            .iter()
            .any(|record| record.context().contains("large_handler"))
    );
    assert!(
        functions
            .iter()
            .any(|record| record.context().contains("medium_handler"))
    );
    assert!(
        !functions
            .iter()
            .any(|record| record.context().contains("small_helper"))
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.context().contains("ignored_large"))
    );
    Ok(())
}

fn scan_function_contexts(source: &str) -> miette::Result<Vec<String>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write snippet")?;
    Ok(scan_modularity_rust_source(
        source,
        &file,
        fixture.path(),
        fixture.path(),
        &test_thresholds(),
    )
    .into_diagnostic()
    .wrap_err("scan")?
    .into_iter()
    .filter(|record| record.kind() == ModularityKind::Function)
    .map(|record| record.context().clone())
    .collect())
}

fn padded_fn_body(lines: usize) -> String {
    let mut body = String::from("{\n");
    for index in 0..lines.saturating_sub(2) {
        body.push_str(&format!("    let _ = {index};\n"));
    }
    body.push_str("}\n");
    body
}

#[test]
fn long_impl_method_is_flagged_on_the_type() -> miette::Result<()> {
    cordial::init_tracing();
    let source = format!(
        "pub struct Widget;\nimpl Widget {{\n    pub fn run(&self) {}\n}}\n",
        padded_fn_body(12)
    );
    let contexts = scan_function_contexts(&source)?;
    assert!(
        contexts
            .iter()
            .any(|context| context.contains("Widget::run")),
        "impl methods should be attributed to the type: {contexts:?}"
    );
    Ok(())
}

#[test]
fn long_trait_default_method_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = format!(
        "pub trait Drive {{\n    fn go(&self) {}\n    fn stop(&self);\n}}\n",
        padded_fn_body(12)
    );
    let contexts = scan_function_contexts(&source)?;
    assert!(
        contexts.iter().any(|context| context.contains("Drive::go")),
        "trait default bodies should flag: {contexts:?}"
    );
    assert!(
        !contexts.iter().any(|context| context.contains("stop")),
        "signature-only trait methods have no body to split: {contexts:?}"
    );
    Ok(())
}

#[test]
fn cfg_test_methods_are_ignored() -> miette::Result<()> {
    cordial::init_tracing();
    let source = format!(
        "pub struct Widget;\n#[cfg(test)]\nimpl Widget {{\n    fn run(&self) {}\n}}\n",
        padded_fn_body(12)
    );
    let contexts = scan_function_contexts(&source)?;
    assert!(
        !contexts.iter().any(|context| context.contains("run")),
        "cfg(test) impls are not production bodies: {contexts:?}"
    );
    Ok(())
}

#[test]
fn function_length_counts_the_body_not_the_signature() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
        pub fn wide<
            A,
            B,
            C,
            D,
            E,
            F,
            G,
            H,
        >() {
            let _ = 1;
        }
    "#;
    let contexts = scan_function_contexts(source)?;
    assert!(
        !contexts.iter().any(|context| context.contains("wide")),
        "a short body must not flag even if the signature is tall: {contexts:?}"
    );
    Ok(())
}

#[test]
fn modularity_default_thresholds() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert_eq!(thresholds.file_inventory_min_lines(), 500);
    assert_eq!(thresholds.function_inventory_min_lines(), 150);
    assert_eq!(thresholds.function_hotspot_min_lines(), 80);
    assert_eq!(thresholds.file_checklist_min_lines(), 500);
    assert_eq!(thresholds.function_checklist_min_lines(), 200);
    assert_eq!(thresholds.max_types_per_file(), 10);
    assert_eq!(thresholds.module_size_sigma(), 2);
    assert!(!thresholds.module_size_ignore_lower_tail());
    assert_eq!(thresholds.min_module_lines(), 0);
    assert_eq!(thresholds.top_heavy_min_percent(), 50);
    assert_eq!(thresholds.lopsided_min_percent(), 75);
    assert_eq!(thresholds.hierarchy_min_lines(), 150);
}
