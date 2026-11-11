//! Module size against the sigma sample: outliers, tails, even sizes, and the minimum module size.

use cordial::{
    MODULARITY_ETIQUETTE, ModularityKind, ModularityThresholds, ModuleSizeStats, RunAll, Session,
    SessionBuilder, scan_modularity_rust_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/modularity_thresholds.rs"]
mod modularity_thresholds;

#[path = "support/modularity_padded_source.rs"]
mod modularity_padded_source;

#[path = "support/modularity_padded_module.rs"]
mod modularity_padded_module;

#[path = "support/modularity_finding_field.rs"]
mod modularity_finding_field;

use modularity_finding_field::field;
use modularity_padded_module::padded_module;
use modularity_padded_source::pad_source_to_lines;
use modularity_thresholds::test_thresholds;

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
fn module_size_zscores_are_diagnostic_only() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(
        !thresholds.is_module_size_checklist(),
        "upper-tail module-size outliers must stay diagnostic"
    );
    assert!(
        !thresholds.is_module_size_checklist(),
        "lower-tail module-size outliers must stay diagnostic"
    );
    let ignore_lower = thresholds.with_module_size_ignore_lower_tail(true);
    assert!(
        !ignore_lower.is_module_size_checklist(),
        "the lower-tail flag must not make z-scores actionable"
    );
    assert!(
        !ignore_lower.is_module_size_checklist(),
        "ignoring the lower tail must not affect the diagnostic-only upper tail"
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
        .filter(|record| record.kind() == ModularityKind::ModuleSize)
        .collect();
    assert_eq!(modules.len(), 2, "{modules:?}");
    assert!(
        modules.iter().any(|record| record.context() == "<crate>"),
        "{modules:?}"
    );
    assert!(
        modules.iter().any(|record| record.context() == "inner"),
        "{modules:?}"
    );
    assert!(
        modules
            .iter()
            .any(|record| record.context() == "<crate>" && !record.inline())
    );
    assert!(
        modules
            .iter()
            .any(|record| record.context() == "inner" && record.inline())
    );
    Ok(())
}

#[test]
fn module_size_session_reports_two_sigma_outlier_without_checklisting_it() -> miette::Result<()> {
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
    fs::write(fixture.path().join("src/huge.rs"), padded_module(500))
        .into_diagnostic()
        .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    let checklist_outliers: Vec<_> = findings
        .iter()
        .copied()
        .filter(|finding| {
            finding.rule().id() == "MODULARITY-MODULE-SIZE"
                && field(*finding, "checklist").as_deref() == Some("true")
        })
        .collect();
    assert!(
        checklist_outliers.is_empty(),
        "module-size z-scores must not create checklist items: {:?}",
        checklist_outliers
            .iter()
            .map(|finding| field(*finding, "context"))
            .collect::<Vec<_>>()
    );
    assert!(
        findings.iter().any(|finding| {
            finding.rule().id() == "MODULARITY-FILE"
                && field(*finding, "file").is_some_and(|file| file.contains("huge.rs"))
                && field(*finding, "checklist").as_deref() == Some("true")
        }),
        "the same file should still be actionable through the direct file limit"
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
    fs::write(fixture.path().join("src/huge.rs"), padded_module(250))
        .into_diagnostic()
        .wrap_err("huge")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
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
fn module_size_lower_tail_stays_diagnostic_when_not_ignored() -> miette::Result<()> {
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
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
            && field(finding, "context").as_deref() == Some("tiny")
    });
    assert!(
        !flagged,
        "a 2σ-small module must not checklist when z-scores are diagnostic"
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
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let flagged = outcome.findings().any(|finding| {
        finding.rule().id() == "MODULARITY-MODULE-SIZE"
            && field(finding, "checklist").as_deref() == Some("true")
            && field(finding, "context").as_deref() == Some("tiny")
    });
    assert!(
        !flagged,
        "module_size_ignore_lower_tail must keep the small-side item diagnostic-only"
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
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
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
        .register(&*MODULARITY_ETIQUETTE)
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
