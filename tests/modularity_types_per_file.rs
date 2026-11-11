//! Types per file: the limit, what is not counted, the configured maximum, and the CSV quoting of the packed names.

use cordial::{
    MODULARITY_ETIQUETTE, ModularityKind, ModularityThresholds, RunAll, Session, SessionBuilder,
    scan_modularity_rust_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/modularity_thresholds.rs"]
mod modularity_thresholds;

use modularity_thresholds::test_thresholds;

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
    .map(|record| record.kind())
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
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
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
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
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
        .register(&*MODULARITY_ETIQUETTE)
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
