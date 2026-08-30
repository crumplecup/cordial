use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    Finding, MODULARITY_ETIQUETTE, MapFindingSink, ModularityKind, ModularityThresholds,
    ModuleHierarchyNode, ModuleSizeInput, ModuleSizeStats, RunAll, Session, SessionBuilder,
    build_module_hierarchy, fat_leaves, library_branches, lopsided_siblings, order_bands,
    scan_modularity_rust_source, top_heavy_parents, unary_nests,
};

fn test_thresholds() -> ModularityThresholds {
    ModularityThresholds::default()
        .with_file_inventory_min_lines(10)
        .with_function_inventory_min_lines(5)
        .with_function_hotspot_min_lines(5)
        .with_file_checklist_min_lines(20)
        .with_function_checklist_min_lines(15)
        .with_max_types_per_file(1)
        .with_lopsided_min_percent(60)
        .with_hierarchy_min_lines(0)
}

const HANDLERS_RS: &str =
    include_str!("../../elicit_doc/tests/fixtures/quality/modularity_crate/src/handlers.rs");

fn large_function_fixture() -> String {
    function_with_body_lines("oversized", 202)
}

fn function_with_body_lines(name: &str, body_lines: u32) -> String {
    let inner = body_lines.saturating_sub(2);
    let mut body = format!("pub fn {name}() {{\n");
    for index in 0..inner {
        body.push_str(&format!("    let _ = {index};\n"));
    }
    body.push_str("}\n");
    body
}

fn pad_source_to_lines(mut source: String, lines: usize) -> String {
    let current = source.lines().count();
    for _ in current..lines {
        source.push_str("// pad\n");
    }
    source
}

#[test]
fn modularity_etiquette_detects_large_functions() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), large_function_fixture())
        .into_diagnostic()
        .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
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
        .filter(|record| record.kind == ModularityKind::Function)
        .collect();
    assert!(
        functions
            .iter()
            .any(|record| record.context.contains("large_handler"))
    );
    assert!(
        functions
            .iter()
            .any(|record| record.context.contains("medium_handler"))
    );
    assert!(
        !functions
            .iter()
            .any(|record| record.context.contains("small_helper"))
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.context.contains("ignored_large"))
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
    .filter(|record| record.kind == ModularityKind::Function)
    .map(|record| record.context)
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
    assert_eq!(thresholds.file_checklist_min_lines(), 1000);
    assert_eq!(thresholds.function_checklist_min_lines(), 200);
    assert_eq!(thresholds.max_types_per_file(), 10);
    assert_eq!(thresholds.module_size_sigma(), 2);
    assert!(!thresholds.module_size_ignore_lower_tail());
    assert_eq!(thresholds.min_module_lines(), 0);
    assert_eq!(thresholds.top_heavy_min_percent(), 50);
    assert_eq!(thresholds.lopsided_min_percent(), 75);
    assert_eq!(thresholds.hierarchy_min_lines(), 150);
}

fn scan_snippet(
    source: &str,
    thresholds: ModularityThresholds,
) -> miette::Result<Vec<ModularityKind>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write snippet")?;
    Ok(scan_modularity_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        &thresholds,
    )
    .into_diagnostic()
    .wrap_err("scan")?
    .into_iter()
    .map(|record| record.kind)
    .collect())
}

#[test]
fn types_per_file_flags_when_over_max() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_snippet("pub struct Alpha;\npub struct Beta;\n", test_thresholds())?;
    assert!(
        findings.contains(&ModularityKind::TypesPerFile),
        "two types exceed max_types_per_file=1: {findings:?}"
    );
    Ok(())
}

#[test]
fn types_per_file_allows_one_type() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_snippet("pub struct Only;\n", test_thresholds())?;
    assert!(
        !findings.contains(&ModularityKind::TypesPerFile),
        "one type is the file-per-type default: {findings:?}"
    );
    Ok(())
}

#[test]
fn types_per_file_ignores_aliases_and_cfg_test() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
        pub struct Only;
        pub type Alias = Only;
        #[cfg(test)]
        pub struct TestOnly;
        #[cfg(test)]
        mod tests {
            pub struct Helper;
        }
    "#;
    let findings = scan_snippet(source, test_thresholds())?;
    assert!(
        !findings.contains(&ModularityKind::TypesPerFile),
        "aliases and cfg(test) types must not count: {findings:?}"
    );
    Ok(())
}

#[test]
fn types_per_file_counts_inline_module_types() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
        pub struct Outer;
        mod inner {
            pub enum Kind { A }
        }
    "#;
    let findings = scan_snippet(source, test_thresholds())?;
    assert!(
        findings.contains(&ModularityKind::TypesPerFile),
        "inline module types still live in this file: {findings:?}"
    );
    Ok(())
}

#[test]
fn types_per_file_respects_higher_config_max() -> miette::Result<()> {
    cordial::init_tracing();
    let thresholds = test_thresholds().with_max_types_per_file(3);
    let findings = scan_snippet(
        "pub struct A;\npub enum B { X }\npub trait C {}\n",
        thresholds,
    )?;
    assert!(
        !findings.contains(&ModularityKind::TypesPerFile),
        "three types are allowed when max is 3: {findings:?}"
    );
    Ok(())
}

#[test]
fn modularity_etiquette_session_reads_types_per_file_config() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub struct Alpha;\npub struct Beta;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nmax_types_per_file = 1\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule().id() == "MODULARITY-TYPES-PER-FILE"),
        "session should surface types-per-file from project config"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**2 types**"));
    Ok(())
}

#[test]
fn generated_files_are_exempt_from_file_and_module_size_but_not_types_per_file()
-> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;

    // Twelve types (over the default max_types_per_file of 10) padded past
    // the default file_inventory_min_lines (500) -- written identically
    // into an excepted "generated" module and a plain module of the same
    // shape, so the test proves the exception is scoped by path, not a
    // blanket "big files are fine now" regression.
    let mut oversized = String::new();
    for index in 0..12 {
        oversized.push_str(&format!("pub struct Type{index};\n"));
    }
    let oversized = pad_source_to_lines(oversized, 600);

    fs::write(fixture.path().join("src/generated.rs"), &oversized)
        .into_diagnostic()
        .wrap_err("write generated")?;
    fs::write(fixture.path().join("src/normal.rs"), &oversized)
        .into_diagnostic()
        .wrap_err("write normal")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod generated;\nmod normal;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\ngenerated_files = [\"src/generated.rs\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();

    let file_files: Vec<_> = findings
        .iter()
        .copied()
        .filter(|finding| finding.rule().id() == "MODULARITY-FILE")
        .filter_map(|finding| field(finding, "file"))
        .collect();
    assert!(
        !file_files.iter().any(|file| file.contains("generated.rs")),
        "an excepted generated file must not get a MODULARITY-FILE finding: {file_files:?}"
    );
    assert!(
        file_files.iter().any(|file| file.contains("normal.rs")),
        "the same-shaped, non-excepted file must still get a MODULARITY-FILE finding: {file_files:?}"
    );

    let module_size_contexts: Vec<_> = findings
        .iter()
        .copied()
        .filter(|finding| finding.rule().id() == "MODULARITY-MODULE-SIZE")
        .filter_map(|finding| field(finding, "context"))
        .collect();
    assert!(
        !module_size_contexts.iter().any(|ctx| ctx == "generated"),
        "an excepted generated file's module must not enter the MODULE-SIZE inventory at all: {module_size_contexts:?}"
    );
    assert!(
        module_size_contexts.iter().any(|ctx| ctx == "normal"),
        "the non-excepted module must still enter the MODULE-SIZE inventory: {module_size_contexts:?}"
    );

    let types_files: Vec<_> = findings
        .iter()
        .copied()
        .filter(|finding| finding.rule().id() == "MODULARITY-TYPES-PER-FILE")
        .filter_map(|finding| field(finding, "file"))
        .collect();
    assert!(
        types_files.iter().any(|file| file.contains("generated.rs")),
        "types-per-file is a different signal (per-type, not file LOC) and must still fire \
         on the excepted file: {types_files:?}"
    );

    Ok(())
}

#[test]
fn module_size_stats_flags_two_sigma_outliers() {
    cordial::init_tracing();
    let sizes = [10, 10, 10, 10, 10, 10, 10, 200];
    let stats = ModuleSizeStats::from_lines(&sizes);
    assert!(stats.is_outlier(200, 2));
    assert!(!stats.is_outlier(10, 2));
    assert!(!stats.is_outlier(200, 3));
}

#[test]
fn module_size_stats_need_spread() {
    cordial::init_tracing();
    let stats = ModuleSizeStats::from_lines(&[12, 12, 12]);
    assert!(!stats.is_outlier(12, 2));
    let one = ModuleSizeStats::from_lines(&[40]);
    assert!(!one.is_outlier(40, 2));
}

#[test]
fn module_size_stats_split_upper_and_lower_tails() {
    cordial::init_tracing();
    let sizes = [10, 10, 10, 10, 10, 10, 10, 200];
    let stats = ModuleSizeStats::from_lines(&sizes);
    assert!(stats.is_upper_outlier(200, 2));
    assert!(!stats.is_lower_outlier(200, 2));
    assert!(!stats.is_upper_outlier(10, 2));
    let small = [200, 200, 200, 200, 200, 200, 200, 5];
    let low = ModuleSizeStats::from_lines(&small);
    assert!(low.is_lower_outlier(5, 2));
    assert!(!low.is_upper_outlier(5, 2));
}

#[test]
fn module_size_checklist_floor_is_upper_tail_only() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(thresholds.is_module_size_checklist(500, Some(2.1)));
    assert!(
        !thresholds.is_module_size_checklist(250, Some(2.1)),
        "upper tail below the file floor must not checklist"
    );
    assert!(
        thresholds.is_module_size_checklist(5, Some(-2.1)),
        "lower tail must still checklist when the ignore flag is off"
    );
    let ignore_lower = thresholds.with_module_size_ignore_lower_tail(true);
    assert!(
        !ignore_lower.is_module_size_checklist(5, Some(-2.1)),
        "lower tail must be silent when the ignore flag is on"
    );
    assert!(
        ignore_lower.is_module_size_checklist(500, Some(2.1)),
        "ignoring the lower tail must not affect the upper tail"
    );
}

#[test]
fn module_size_inventory_includes_file_and_inline_mod() -> miette::Result<()> {
    cordial::init_tracing();
    let source = "mod inner {\n    pub fn x() {}\n}\n";
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;
    let records = scan_modularity_rust_source(
        source,
        &file,
        fixture.path(),
        fixture.path(),
        &test_thresholds(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    let modules: Vec<_> = records
        .iter()
        .filter(|record| record.kind == ModularityKind::ModuleSize)
        .collect();
    assert_eq!(modules.len(), 2, "{modules:?}");
    assert!(
        modules.iter().any(|record| record.context == "<crate>"),
        "{modules:?}"
    );
    assert!(
        modules.iter().any(|record| record.context == "inner"),
        "{modules:?}"
    );
    assert!(
        modules
            .iter()
            .any(|record| record.context == "<crate>" && !record.inline)
    );
    assert!(
        modules
            .iter()
            .any(|record| record.context == "inner" && record.inline)
    );
    Ok(())
}

fn padded_module(lines: usize) -> String {
    let mut body = String::from("pub fn x() {}\n");
    for _ in 1..lines {
        body.push_str("// pad\n");
    }
    body
}

#[test]
fn module_size_session_flags_two_sigma_outlier() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/huge.rs"), padded_module(500))
        .into_diagnostic()
        .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    let outliers: Vec<_> = findings
        .iter()
        .copied()
        .filter(|finding| {
            finding.rule().id() == "MODULARITY-MODULE-SIZE"
                && field(*finding, "checklist").as_deref() == Some("true")
        })
        .collect();
    assert!(
        outliers
            .iter()
            .any(|finding| field(*finding, "context").as_deref() == Some("huge")),
        "huge at the file floor should be a 2σ upper-tail checklist item: {:?}",
        outliers
            .iter()
            .map(|finding| field(*finding, "context"))
            .collect::<Vec<_>>()
    );

    let summary = fs::read_to_string(store.path().join("findings/modularity-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("## Module sizes"));
    assert!(summary.contains("`huge`"));
    Ok(())
}

#[test]
fn module_size_upper_tail_below_file_floor_is_not_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/huge.rs"), padded_module(250))
        .into_diagnostic()
        .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
            && field(finding, "context").as_deref() == Some("huge")
    });
    assert!(
        !flagged,
        "a 2σ-large module below the file inventory floor must stay inventory-only"
    );
    let summary = fs::read_to_string(store.path().join("findings/modularity-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(
        summary.contains("`huge`"),
        "the module must still appear in the ranked inventory: {summary}"
    );
    Ok(())
}

#[test]
fn module_size_lower_tail_checklists_when_not_ignored() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(200),
        )
        .into_diagnostic()
        .wrap_err("large sibling")?;
    }
    lib.push_str("mod tiny;\n");
    fs::write(
        fixture.path().join("src/lib.rs"),
        pad_source_to_lines(lib, 200),
    )
    .into_diagnostic()
    .wrap_err("lib")?;
    fs::write(fixture.path().join("src/tiny.rs"), padded_module(5))
        .into_diagnostic()
        .wrap_err("tiny")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
            && field(finding, "context").as_deref() == Some("tiny")
    });
    assert!(
        flagged,
        "a 2σ-small module must checklist when the lower tail is not ignored"
    );
    Ok(())
}

#[test]
fn module_size_lower_tail_can_be_ignored() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(200),
        )
        .into_diagnostic()
        .wrap_err("large sibling")?;
    }
    lib.push_str("mod tiny;\n");
    fs::write(
        fixture.path().join("src/lib.rs"),
        pad_source_to_lines(lib, 200),
    )
    .into_diagnostic()
    .wrap_err("lib")?;
    fs::write(fixture.path().join("src/tiny.rs"), padded_module(5))
        .into_diagnostic()
        .wrap_err("tiny")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nmodule_size_ignore_lower_tail = true\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
            && field(finding, "context").as_deref() == Some("tiny")
    });
    assert!(
        !flagged,
        "module_size_ignore_lower_tail must drop the small-side checklist item"
    );
    Ok(())
}

#[test]
fn module_size_session_does_not_flag_even_sizes() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..5 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(5),
        )
        .into_diagnostic()
        .wrap_err("module")?;
    }
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
    });
    assert!(!flagged, "similar module sizes must not be 2σ outliers");
    Ok(())
}

#[test]
fn min_module_lines_drops_tiny_modules_from_sigma_sample() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/huge.rs"), padded_module(250))
        .into_diagnostic()
        .wrap_err("huge")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nmin_module_lines = 50\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
    });
    assert!(
        !flagged,
        "with min_module_lines=50 only huge remains in the sample, so n=1 and no 2σ lint"
    );
    Ok(())
}

#[test]
fn strahler_order_bumps_when_two_children_share_max() -> miette::Result<()> {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("left", "src/left.rs", 8),
        input("right", "src/right.rs", 8),
        input("left::a", "src/left/a.rs", 4),
        input("left::b", "src/left/b.rs", 4),
    ]);
    let crate_root = node(&nodes, "<crate>")?;
    let left = node(&nodes, "left")?;
    let right = node(&nodes, "right")?;
    assert_eq!(right.order, 1, "single leaf child keeps order 1");
    assert_eq!(left.order, 2, "two order-1 children bump parent to 2");
    assert_eq!(crate_root.order, 2, "max child order 2 with one such child");
    let bands = order_bands(&nodes);
    assert!(bands.iter().any(|band| band.order == 1 && band.count >= 3));
    Ok(())
}

#[test]
fn library_branches_rank_top_heavy_parents_first() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 6),
        input("fat", "src/fat/mod.rs", 80),
        input("fat::leaf", "src/fat/leaf.rs", 8),
        input("thin", "src/thin/mod.rs", 8),
        input("thin::leaf", "src/thin/leaf.rs", 80),
    ]);
    let branches = library_branches(&nodes);
    assert_eq!(
        branches
            .iter()
            .map(|node| node.path.as_str())
            .collect::<Vec<_>>(),
        vec!["fat", "thin"]
    );
    assert!(branches[0].top_heavy() > 0.8);
    assert!(branches[1].top_heavy() < 0.2);
}

#[test]
fn fat_leaves_are_not_ranked_as_library_branches() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 6),
        input("session", "src/session.rs", 695),
        input("fat", "src/fat/mod.rs", 80),
        input("fat::leaf", "src/fat/leaf.rs", 8),
    ]);
    let branches: Vec<_> = library_branches(&nodes)
        .iter()
        .map(|node| node.path.as_str())
        .collect();
    assert_eq!(branches, vec!["fat"]);
    let leaves = fat_leaves(&nodes);
    assert_eq!(leaves[0].path, "session");
    assert!(!leaves.iter().any(|node| node.path == "fat"));
}

#[test]
fn top_heavy_parents_include_nested_nodes() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("pkg", "src/pkg/mod.rs", 8),
        input("pkg::loader", "src/pkg/loader.rs", 90),
        input("pkg::loader::inner", "src/pkg/loader/inner.rs", 8),
    ]);
    let parents = top_heavy_parents(&nodes);
    assert_eq!(parents[0].path, "pkg::loader");
    assert!(parents[0].top_heavy() > 0.8);
}

#[test]
fn lopsided_siblings_rank_the_dominant_child() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("antipatterns", "src/antipatterns.rs", 3000),
        input("trenchcoat", "src/trenchcoat.rs", 300),
    ]);
    let ranked = lopsided_siblings(&nodes, 0);
    assert_eq!(ranked[0].parent, "<crate>");
    assert_eq!(ranked[0].largest, "antipatterns");
    assert!(ranked[0].share > 0.8);
    assert_eq!(ranked[0].sibling_count, 2);
    assert_eq!(ranked[0].sibling_total, 3300);
}

#[test]
fn lopsided_siblings_ignore_stub_children() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("tracked_targets", "src/tracked_targets.rs", 275),
        input("coverage", "src/coverage.rs", 3),
    ]);
    assert!(lopsided_siblings(&nodes, 150).is_empty());
    let ranked = lopsided_siblings(&nodes, 0);
    assert_eq!(ranked[0].largest, "tracked_targets");
    assert!(ranked[0].share > 0.98);
}

#[test]
fn lopsided_hit_requires_seventy_five_percent_of_substantial_siblings() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(!thresholds.is_lopsided_hit(296, 462));
    assert!(thresholds.is_lopsided_hit(450, 600));
}

#[test]
fn unary_nests_rank_the_passthrough_directory() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("error", "src/error.rs", 200),
        input("error::sources", "src/error/sources/mod.rs", 14),
        input("error::sources::io", "src/error/sources/io.rs", 140),
        input("error::sources::local", "src/error/sources/local.rs", 210),
        input("error::sources::parse", "src/error/sources/parse.rs", 200),
    ]);
    let ranked = unary_nests(&nodes, 150);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].parent, "error");
    assert_eq!(ranked[0].passthrough, "error::sources");
    assert_eq!(ranked[0].passthrough_own, 14);
    assert_eq!(ranked[0].grandchildren.len(), 3);
}

#[test]
fn unary_nests_ignore_a_unary_leaf_child() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("chain_layer", "src/chain_layer/mod.rs", 194),
        input("chain_layer::preds", "src/chain_layer/preds.rs", 352),
    ]);
    assert!(unary_nests(&nodes, 150).is_empty());
}

#[test]
fn unary_nests_ignore_crate_root_unary_package() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("only", "src/only/mod.rs", 8),
        input("only::a", "src/only/a.rs", 80),
        input("only::b", "src/only/b.rs", 80),
    ]);
    assert!(unary_nests(&nodes, 150).is_empty());
}

#[test]
fn collapse_hit_requires_hierarchy_min_subtree() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(!thresholds.is_collapse_hit(149));
    assert!(thresholds.is_collapse_hit(150));
}

#[test]
fn top_heavy_parent_is_a_peel_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/fat"))
        .into_diagnostic()
        .wrap_err("fat")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod fat;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/fat/mod.rs"),
        format!("mod leaf;\n{}", padded_module(160)),
    )
    .into_diagnostic()
    .wrap_err("fat")?;
    fs::write(fixture.path().join("src/fat/leaf.rs"), padded_module(8))
        .into_diagnostic()
        .wrap_err("leaf")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-TOP-HEAVY"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("fat")
        }),
        "fat should be a peel-the-parent hit"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("peel `fat`"),
        "checklist should name the peel action: {checklist}"
    );
    Ok(())
}

#[test]
fn lopsided_sibling_is_a_split_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod big;\nmod small;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/big.rs"), padded_module(200))
        .into_diagnostic()
        .wrap_err("big")?;
    fs::write(fixture.path().join("src/small.rs"), padded_module(60))
        .into_diagnostic()
        .wrap_err("small")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-LOPSIDED"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("big")
        }),
        "big should be the dominant sibling to split"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("split `big`"),
        "checklist should name the split-dominant action: {checklist}"
    );
    Ok(())
}

#[test]
fn unary_nest_is_a_collapse_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error/sources"))
        .into_diagnostic()
        .wrap_err("sources")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/error.rs"),
        format!("mod sources;\n{}", padded_module(40)),
    )
    .into_diagnostic()
    .wrap_err("error")?;
    fs::write(
        fixture.path().join("src/error/sources/mod.rs"),
        "mod io;\nmod local;\n",
    )
    .into_diagnostic()
    .wrap_err("sources mod")?;
    fs::write(
        fixture.path().join("src/error/sources/io.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("io")?;
    fs::write(
        fixture.path().join("src/error/sources/local.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("local")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-COLLAPSE"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("error::sources")
        }),
        "error::sources should be a collapse hit"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("collapse `error::sources`"),
        "checklist should name the collapse action: {checklist}"
    );
    Ok(())
}

#[test]
fn unary_leaf_is_not_a_collapse_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/chain_layer"))
        .into_diagnostic()
        .wrap_err("chain_layer")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod chain_layer;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/chain_layer/mod.rs"),
        format!("mod preds;\n{}", padded_module(40)),
    )
    .into_diagnostic()
    .wrap_err("mod")?;
    fs::write(
        fixture.path().join("src/chain_layer/preds.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("preds")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        !outcome
            .findings()
            .any(|finding| finding.rule().id() == "MODULARITY-COLLAPSE"),
        "a unary leaf child is a peel, not a collapse"
    );
    Ok(())
}

#[test]
fn checklist_names_longest_methods_on_too_long_files() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/huge.rs"),
        pad_source_to_lines(large_function_fixture(), 500),
    )
    .into_diagnostic()
    .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("### Too long"),
        "size outliers should be one hotspot list: {checklist}"
    );
    assert!(
        checklist.contains("split `huge::oversized`"),
        "hotspot should name the body to split: {checklist}"
    );
    assert!(
        checklist.contains("or grow a subtree"),
        "too-long fat leaves should offer grow-a-subtree after helpers: {checklist}"
    );
    assert!(
        !checklist.contains("### Split these bodies"),
        "nested method must not be duplicated as its own item: {checklist}"
    );
    let summary = fs::read_to_string(store.path().join("findings/modularity-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("## Longest method bodies"));
    assert!(summary.contains("huge::oversized"));
    Ok(())
}

#[test]
fn checklist_extract_helpers_on_too_long_files_without_long_bodies() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/huge.rs"), padded_module(1000))
        .into_diagnostic()
        .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("extract helpers"),
        "too-long files with no long body should say extract helpers: {checklist}"
    );
    assert!(
        checklist.contains("or grow a subtree"),
        "fat-leaf helper extraction should still offer a named-layer split: {checklist}"
    );
    assert!(
        !checklist.contains("split `huge"),
        "must not invent a body-split when no method is over the floor: {checklist}"
    );
    Ok(())
}

#[test]
fn checklist_names_helpers_below_inventory_on_too_long_files() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    let mut lib = String::new();
    for index in 0..7 {
        lib.push_str(&format!("mod m{index};\n"));
        fs::write(
            fixture.path().join(format!("src/m{index}.rs")),
            padded_module(8),
        )
        .into_diagnostic()
        .wrap_err("small module")?;
    }
    lib.push_str("mod huge;\n");
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/huge.rs"),
        pad_source_to_lines(function_with_body_lines("medium", 90), 1000),
    )
    .into_diagnostic()
    .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("extract helpers from `huge::medium`"),
        "too-long files should name helper-sized bodies: {checklist}"
    );
    let csv = fs::read_to_string(store.path().join("findings/modularity.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        !csv.contains("huge::medium"),
        "helper-sized bodies must not enter CSV inventory: {csv}"
    );
    Ok(())
}

#[test]
fn scan_records_helpers_only_on_inventory_sized_files() -> miette::Result<()> {
    cordial::init_tracing();
    let helper = function_with_body_lines("helper", 90);
    let small = scan_modularity_rust_source(
        &helper,
        std::path::Path::new("lib.rs"),
        std::path::Path::new("."),
        std::path::Path::new("."),
        &ModularityThresholds::default(),
    )
    .into_diagnostic()
    .wrap_err("scan small")?;
    assert!(
        !small
            .iter()
            .any(|record| record.kind == ModularityKind::Function),
        "a 90-line body on a small file is below inventory: {small:?}"
    );

    let large = pad_source_to_lines(helper, 500);
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, &large)
        .into_diagnostic()
        .wrap_err("write")?;
    let scanned = scan_modularity_rust_source(
        &large,
        &file,
        fixture.path(),
        fixture.path(),
        &ModularityThresholds::default(),
    )
    .into_diagnostic()
    .wrap_err("scan large")?;
    assert!(
        scanned.iter().any(|record| {
            record.kind == ModularityKind::Function
                && record.context.contains("helper")
                && record.lines >= 80
                && record.lines < 150
        }),
        "inventory-sized files should record helper-sized bodies: {scanned:?}"
    );
    Ok(())
}

#[test]
fn module_hierarchy_session_writes_branch_ranking() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/fat"))
        .into_diagnostic()
        .wrap_err("fat")?;
    fs::create_dir_all(fixture.path().join("src/thin"))
        .into_diagnostic()
        .wrap_err("thin")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod fat;\nmod thin;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/fat/mod.rs"),
        format!("mod leaf;\n{}", padded_module(80)),
    )
    .into_diagnostic()
    .wrap_err("fat")?;
    fs::write(fixture.path().join("src/fat/leaf.rs"), padded_module(8))
        .into_diagnostic()
        .wrap_err("fat leaf")?;
    fs::write(
        fixture.path().join("src/thin/mod.rs"),
        format!("mod leaf;\n{}", padded_module(8)),
    )
    .into_diagnostic()
    .wrap_err("thin")?;
    fs::write(fixture.path().join("src/thin/leaf.rs"), padded_module(80))
        .into_diagnostic()
        .wrap_err("thin leaf")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let summary = fs::read_to_string(store.path().join("findings/modularity-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("## Stream order"));
    assert!(summary.contains("## Library branches"));
    assert!(summary.contains("## Fat leaves"));
    assert!(summary.contains("## Top-heavy parents"));
    assert!(summary.contains("## Lopsided siblings"));
    assert!(summary.contains("## Unary nests"));
    let fat_pos = summary
        .find("| `fat`")
        .ok_or_else(|| miette::miette!("fat branch"))?;
    let thin_pos = summary
        .find("| `thin`")
        .ok_or_else(|| miette::miette!("thin branch"))?;
    assert!(fat_pos < thin_pos, "fat (top-heavy) should rank above thin");
    let csv = fs::read_to_string(store.path().join("findings/modularity-branches.csv"))
        .into_diagnostic()
        .wrap_err("branches csv")?;
    assert!(csv.contains("fat,"));
    assert!(csv.contains("thin,"));
    Ok(())
}

fn input(path: &str, file: &str, lines: u32) -> ModuleSizeInput {
    ModuleSizeInput {
        path: path.to_string(),
        file: file.to_string(),
        lines,
    }
}

fn node<'a>(
    nodes: &'a [ModuleHierarchyNode],
    path: &str,
) -> miette::Result<&'a ModuleHierarchyNode> {
    nodes
        .iter()
        .find(|node| node.path == path)
        .ok_or_else(|| miette::miette!("missing {path}"))
}

#[test]
fn types_per_file_csv_row_quotes_the_comma_joined_context() -> miette::Result<()> {
    // A real bug, found via dogfooding: MODULARITY-TYPES-PER-FILE's
    // `context` field joins every packed type name with ", ", and used
    // to be written into modularity.csv unquoted -- any file with 2+
    // types silently shifted every column after `context` for that row,
    // since a plain CSV reader treats each embedded comma as a real
    // column separator.
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub struct TypeOne;\npub struct TypeTwo;\npub struct TypeThree;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nmax_types_per_file = 1\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/modularity.csv"))
        .into_diagnostic()
        .wrap_err("read modularity.csv")?;
    let row = csv
        .lines()
        .find(|line| line.contains("MODULARITY-TYPES-PER-FILE"))
        .ok_or_else(|| miette::miette!("no MODULARITY-TYPES-PER-FILE row in: {csv}"))?;

    assert!(
        row.contains("\"TypeOne, TypeTwo, TypeThree\""),
        "context field must be quoted so its own commas don't shift columns: {row}"
    );
    assert_eq!(
        quote_aware_column_count(row),
        10,
        "row must still have exactly 10 columns despite the comma-bearing context field: {row}"
    );
    Ok(())
}

/// Minimal RFC 4180-aware comma split, test-only: counts columns the way
/// a real CSV reader would, respecting quoted fields — a naive
/// `row.split(',').count()` can't tell a quoted field's internal comma
/// from a real column separator, which is exactly the distinction this
/// test exists to check.
fn quote_aware_column_count(row: &str) -> usize {
    let mut count = 1;
    let mut in_quotes = false;
    for c in row.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => count += 1,
            _ => {}
        }
    }
    count
}

fn field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
