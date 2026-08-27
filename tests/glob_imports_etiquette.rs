use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    GLOB_IMPORTS_ETIQUETTE, GlobImportRuleId, RunAll, Session, SessionBuilder,
    scan_glob_imports_rust_source,
};

const GLOB_SAMPLE: &str = r#"
use std::collections::*;
use std::io::{self, *};
use std::path::Path;

pub use crate::inner::*;

fn clean() {
    let _ = Path::new(".");
}
"#;

#[test]
fn scan_glob_imports_finds_three_stars() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("globs.rs");
    fs::write(&file, GLOB_SAMPLE)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_glob_imports_rust_source(
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
            .filter(|record| record.rule_id == GlobImportRuleId::Import001)
            .count(),
        3
    );
    assert!(
        findings
            .iter()
            .any(|record| record.snippet == "std::collections::*")
    );
    assert!(findings.iter().any(|record| record.snippet == "std::io::*"));
    assert!(
        findings
            .iter()
            .any(|record| record.snippet == "crate::inner::*")
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.snippet.contains("Path"))
    );
    Ok(())
}

#[test]
fn glob_imports_etiquette_writes_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), GLOB_SAMPLE)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&GLOB_IMPORTS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 3);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("glob-imports.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("GLOB-IMPORT-001"));
    assert!(csv.contains("std::collections::*"));

    let checklist = fs::read_to_string(findings_dir.join("glob-imports.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 3"));
    Ok(())
}

#[test]
fn glob_imports_in_tests_tree_count() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::create_dir_all(fixture.path().join("tests"))
        .into_diagnostic()
        .wrap_err("tests")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("tests/it.rs"),
        "use crate::helper::*;\n",
    )
    .into_diagnostic()
    .wrap_err("it")?;

    let records = cordial::scan_crate_glob_imports(fixture.path())
        .into_diagnostic()
        .wrap_err("scan crate")?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].snippet, "crate::helper::*");
    Ok(())
}

#[test]
fn super_glob_and_cfg_test_module_globs_are_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
mod child {
    use super::*;
}

#[cfg(test)]
mod tests {
    use std::collections::*;
}

use crate::inner::*;
"#;
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("globs.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;

    let findings = scan_glob_imports_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    let snippets: Vec<_> = findings.iter().map(|row| row.snippet.as_str()).collect();
    assert_eq!(
        snippets,
        ["super::*", "std::collections::*", "crate::inner::*"]
    );
    Ok(())
}

#[test]
fn sibling_super_glob_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = "mod child { use super::helper::*; }\n";
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sib.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;

    let findings = scan_glob_imports_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].snippet, "super::helper::*");
    Ok(())
}
