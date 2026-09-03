use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    Disposition, PrintRuleId, RunAll, Session, SessionBuilder, TRACING_ETIQUETTE,
    TracingStdioPolicy, scan_crate_tracing_print, scan_tracing_print_rust_source,
};

fn scan_source_with(source: &str, policy: &TracingStdioPolicy) -> miette::Result<Vec<PrintRuleId>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("src").join("lib.rs");
    fs::create_dir_all(file.parent().ok_or_else(|| miette::miette!("src"))?)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;
    let records =
        scan_tracing_print_rust_source(source, &file, fixture.path(), fixture.path(), policy)
            .into_diagnostic()
            .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id()).collect())
}

fn scan_source(source: &str) -> miette::Result<Vec<PrintRuleId>> {
    scan_source_with(source, &TracingStdioPolicy::default())
}

fn write_tree(files: &[(&str, &str)]) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    for (rel, source) in files {
        let path = fixture.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err("parent dir")?;
        }
        fs::write(&path, source)
            .into_diagnostic()
            .wrap_err("write file")?;
    }
    Ok(fixture)
}

fn snippets_with(root: &Path, policy: &TracingStdioPolicy) -> miette::Result<Vec<String>> {
    let records = scan_crate_tracing_print(root, policy)
        .into_diagnostic()
        .wrap_err("scan crate")?;
    Ok(records
        .into_iter()
        .map(|record| record.snippet().clone())
        .collect())
}

fn snippets(root: &Path) -> miette::Result<Vec<String>> {
    snippets_with(root, &TracingStdioPolicy::default())
}

#[test]
fn library_println_and_eprintln_fire() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_source(
        r#"
pub fn noisy() {
    println!("hello");
    eprintln!("oops");
}
"#,
    )?;
    assert_eq!(ids, vec![PrintRuleId::Println, PrintRuleId::Eprintln]);
    Ok(())
}

#[test]
fn std_println_fires() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_source(
        r#"
pub fn qualified() {
    std::println!("hello");
}
"#,
    )?;
    assert_eq!(ids, vec![PrintRuleId::Println]);
    Ok(())
}

#[test]
fn print_eprint_and_dbg_fire() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_source(
        r#"
pub fn other_stdio() {
    print!("partial");
    eprint!("partial err");
    dbg!(1);
}
"#,
    )?;
    assert_eq!(
        ids,
        vec![PrintRuleId::Print, PrintRuleId::Eprint, PrintRuleId::Dbg]
    );
    Ok(())
}

#[test]
fn cargo_protocol_prints_are_skipped_by_default() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_source(
        r#"
pub fn build_scriptish() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo::rerun-if-changed=src");
    println!("not cargo");
}
"#,
    )?;
    assert_eq!(ids, vec![PrintRuleId::Println]);
    Ok(())
}

#[test]
fn cargo_protocol_fires_when_skip_is_off() -> miette::Result<()> {
    cordial::init_tracing();
    let policy = TracingStdioPolicy::new(
        true,
        true,
        true,
        true,
        true,
        false,
        vec!["tests/fixtures".into(), "tests/parity".into()],
    );
    let ids = scan_source_with(
        r#"
pub fn build_scriptish() {
    println!("cargo:rerun-if-changed=src");
}
"#,
        &policy,
    )?;
    assert_eq!(ids, vec![PrintRuleId::Println]);
    Ok(())
}

#[test]
fn dbg_knob_off_silences_only_dbg() -> miette::Result<()> {
    cordial::init_tracing();
    let policy = TracingStdioPolicy::new(
        true,
        true,
        true,
        true,
        false,
        true,
        vec!["tests/fixtures".into(), "tests/parity".into()],
    );
    let ids = scan_source_with(
        r#"
pub fn mixed() {
    println!("ok");
    dbg!(1);
}
"#,
        &policy,
    )?;
    assert_eq!(ids, vec![PrintRuleId::Println]);
    Ok(())
}

#[test]
fn skip_folders_skips_that_tree() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_tree(&[
        ("src/lib.rs", "pub fn leftover() { println!(\"lib\"); }\n"),
        (
            "src/generated/mod.rs",
            "pub fn generated() { println!(\"gen\"); }\n",
        ),
    ])?;
    let policy = TracingStdioPolicy::new(
        true,
        true,
        true,
        true,
        true,
        true,
        vec!["src/generated".into()],
    );
    let found = snippets_with(fixture.path(), &policy)?;
    assert_eq!(found, vec!["println!".to_string()]);
    Ok(())
}

#[test]
fn main_bin_cli_and_tests_fire() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_tree(&[
        ("src/lib.rs", "pub fn leftover() { println!(\"lib\"); }\n"),
        (
            "src/cli/run.rs",
            "pub fn report() { eprintln!(\"cli\"); }\n",
        ),
        ("src/main.rs", "fn main() { println!(\"main\"); }\n"),
        ("src/bin/tool.rs", "fn main() { eprintln!(\"bin\"); }\n"),
        (
            "tests/smoke.rs",
            "#[test] fn it_works() { println!(\"test\"); }\n",
        ),
    ])?;
    let found = snippets(fixture.path())?;
    assert_eq!(
        found,
        vec![
            "eprintln!".to_string(),
            "eprintln!".to_string(),
            "println!".to_string(),
            "println!".to_string(),
            "println!".to_string(),
        ]
    );
    Ok(())
}

#[test]
fn session_writes_print_checklist_not_instrument_rows() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_tree(&[("src/lib.rs", "pub fn leftover() { println!(\"lib\"); }\n")])?;
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let print: Vec<_> = outcome
        .findings()
        .filter(|finding| {
            finding.disposition() == Disposition::Open
                && PrintRuleId::is_print_rule(finding.rule().id())
        })
        .collect();
    assert_eq!(print.len(), 1, "session should emit TRACING-STD-PRINTLN");
    assert_eq!(print[0].rule().id(), PrintRuleId::Println.as_str());

    let findings_dir = store.path().join("findings");
    let instrument = fs::read_to_string(findings_dir.join("tracing-instrument.checklist.md"))
        .into_diagnostic()
        .wrap_err("instrument checklist")?;
    assert!(
        !instrument.contains("TRACING-STD-"),
        "instrument checklist must not swallow print rows: {instrument}"
    );
    let checklist = fs::read_to_string(findings_dir.join("tracing-print.checklist.md"))
        .into_diagnostic()
        .wrap_err("print checklist")?;
    assert!(checklist.contains("TRACING-STD-PRINTLN"));
    assert!(checklist.contains("**Open items:**"));
    Ok(())
}

#[test]
fn session_honors_stdio_toml_knobs() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_tree(&[(
        "src/lib.rs",
        "pub fn leftover() { println!(\"lib\"); dbg!(1); }\n",
    )])?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[tracing.stdio]\ndbg = false\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&TRACING_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let print: Vec<_> = outcome
        .findings()
        .filter(|finding| {
            finding.disposition() == Disposition::Open
                && PrintRuleId::is_print_rule(finding.rule().id())
        })
        .map(|finding| finding.rule().id().to_string())
        .collect();
    assert_eq!(print, vec![PrintRuleId::Println.as_str()]);
    Ok(())
}

#[test]
fn dogfood_cordial_library_src_has_no_leftover_prints() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let found = snippets(root)?;
    assert!(
        found.is_empty(),
        "cordial src/ and tests/ should use tracing, not leftover stdio: {found:?}"
    );
    Ok(())
}
