//! File size: generated files are exempt, the checklist lists files by size and names the methods to extract.

#[cfg(feature = "quality")]
use cordial::{ChecklistSource, CordialConfig, StoreLayout, Worklist, build_quality_report};
use cordial::{
    MODULARITY_ETIQUETTE, ModularityKind, ModularityThresholds, RunAll, Session, SessionBuilder,
    scan_modularity_rust_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/modularity_function_fixtures.rs"]
mod modularity_function_fixtures;

#[path = "support/modularity_padded_source.rs"]
mod modularity_padded_source;

#[path = "support/modularity_padded_module.rs"]
mod modularity_padded_module;

#[path = "support/modularity_finding_field.rs"]
mod modularity_finding_field;

use modularity_finding_field::field;
use modularity_function_fixtures::{function_with_body_lines, large_function_fixture};
use modularity_padded_module::padded_module;
use modularity_padded_source::pad_source_to_lines;

#[test]
fn generated_files_are_exempt_from_file_and_module_size_but_not_types_per_file()
-> miette::Result<()> {
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
        .register(&*MODULARITY_ETIQUETTE)
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
fn checklist_lists_over_limit_files_by_descending_size() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod big;\nmod medium;\nmod small;\n",
    )
    .into_diagnostic()
    .wrap_err("lib")?;
    fs::write(fixture.path().join("src/big.rs"), padded_module(650))
        .into_diagnostic()
        .wrap_err("big")?;
    fs::write(fixture.path().join("src/medium.rs"), padded_module(550))
        .into_diagnostic()
        .wrap_err("medium")?;
    fs::write(fixture.path().join("src/small.rs"), padded_module(50))
        .into_diagnostic()
        .wrap_err("small")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("### Files over limit"),
        "checklist should name the direct file-size queue: {checklist}"
    );
    let big = checklist
        .find("big.rs")
        .ok_or_else(|| miette::miette!("missing big file in checklist"))?;
    let medium = checklist
        .find("medium.rs")
        .ok_or_else(|| miette::miette!("missing medium file in checklist"))?;
    assert!(
        big < medium,
        "files over the limit should be sorted largest first: {checklist}"
    );
    assert!(
        !checklist.contains("z="),
        "z-scores should stay out of the action queue copy: {checklist}"
    );
    Ok(())
}

#[test]
#[cfg(feature = "quality")]
fn nested_file_findings_count_as_one_worklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let mut source = String::new();
    for index in 0..11 {
        source.push_str(&format!("pub struct Type{index};\n"));
    }
    fs::write(
        fixture.path().join("src/lib.rs"),
        pad_source_to_lines(source, 600),
    )
    .into_diagnostic()
    .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(findings.iter().any(|finding| {
        finding.rule().id() == "MODULARITY-FILE"
            && field(*finding, "checklist").as_deref() == Some("true")
    }));
    assert!(findings.iter().any(|finding| {
        finding.rule().id() == "MODULARITY-TYPES-PER-FILE"
            && field(*finding, "checklist").as_deref() == Some("true")
    }));

    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert_eq!(checklist.matches("- [ ] ").count(), 1, "{checklist}");
    assert!(
        checklist.contains("peel types: **11 types**"),
        "{checklist}"
    );

    let report = build_quality_report(&findings).into_diagnostic()?;
    let modularity = report
        .areas()
        .iter()
        .find(|area| area.title() == "Modularity")
        .ok_or_else(|| miette::miette!("missing Modularity area"))?;
    assert_eq!(modularity.open_items(), 1);
    let layout = StoreLayout::from_root(store.path(), "modularity_fixture");
    let list = Worklist::build(
        &report,
        &layout,
        &ChecklistSource::new(&layout),
        &CordialConfig::default(),
    )
    .into_diagnostic()?;
    assert_eq!(list.items().len(), 1);
    Ok(())
}

#[test]
fn checklist_names_longest_methods_on_too_long_files() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("### Files over limit"),
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
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
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
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
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
            .any(|record| record.kind() == ModularityKind::Function),
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
            record.kind() == ModularityKind::Function
                && record.context().contains("helper")
                && record.lines() >= 80
                && record.lines() < 150
        }),
        "inventory-sized files should record helper-sized bodies: {scanned:?}"
    );
    Ok(())
}
