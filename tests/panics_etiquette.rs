use std::fs;

use cordial::{
    ErrorSurface, PANICS_ETIQUETTE, PanicKind, RunAll, Session, SessionBuilder,
    project_slug_from_path,
};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn panics_etiquette_detects_panic_expect_and_unreachable() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub fn boom() {
    panic!("kaboom");
}

pub fn fragile() -> u32 {
    Some(1).expect("missing").unwrap()
}

pub fn never() -> ! {
    unreachable!("nope");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 4);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("panics.csv"))
        .into_diagnostic()
        .wrap_err("panics csv")?;
    assert!(csv.contains("PANIC-SOURCE-PANIC"));
    assert!(csv.contains("PANIC-SOURCE-EXPECT"));
    assert!(csv.contains("PANIC-SOURCE-UNREACHABLE"));

    let checklist = fs::read_to_string(findings_dir.join("panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("panic!"));
    assert!(checklist.contains("Library — return internal error types"));
    assert!(checklist.contains("internal error type"));

    let summary = fs::read_to_string(findings_dir.join("panics-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("| 4 | 1 | 1 | 1 | 1 | 0 |"));
    assert!(summary.contains("unwrap **1**"));

    let slug = project_slug_from_path(fixture.path());
    assert!(
        store
            .path()
            .join("cache")
            .join(format!("{slug}.ir.json"))
            .is_file()
    );
    Ok(())
}

#[test]
fn panics_disabled_in_cordial_toml_skips_the_etiquette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn boom() { panic!(\"kaboom\"); }\n",
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[panics]\nenabled = false\n",
    )
    .into_diagnostic()
    .wrap_err("cordial.toml")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 0);
    assert!(
        !store.path().join("findings/panics.csv").is_file(),
        "disabled etiquette must not write findings"
    );
    Ok(())
}

#[test]
fn scan_rust_source_finds_panics_inside_a_verus_block() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
use verus_builtin_macros::verus;

verus! {

pub fn verify_try_from_int_error_occurs_exactly_when_out_of_range(value: i32) -> (result: bool)
    ensures
        result,
{
    match <u8 as std::convert::TryFrom<i32>>::try_from(value) {
        Ok(converted) => (0 <= value && value <= u8::MAX as i32) && converted as i32 == value,
        Err(_) => value < 0 || value > u8::MAX as i32,
    }
}

pub fn verify_int_error_kind_classifies_parse_failures(s: &str) -> (result: bool)
    requires
        s@.len() == 0,
    ensures
        result,
{
    match <i32 as std::str::FromStr>::from_str(s) {
        Ok(_) => unreachable!(),
        Err(_) => true,
    }
}

// A real caller: without one, this fn is an ensures-bearing verification
// leaf (see verus_reach), correctly exempt on its own -- this fixture's
// own point is that panics ARE still found inside verus! blocks, so it
// needs a non-leaf example.
pub fn calls_the_classifier(s: &str) -> bool {
    verify_int_error_kind_classifies_parse_failures(s)
}

}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    assert_eq!(
        findings.len(),
        1,
        "expected exactly the one real unreachable!() site: {findings:?}"
    );
    assert_eq!(findings[0].kind, PanicKind::Unreachable);
    assert!(
        findings[0]
            .context
            .ends_with("verify_int_error_kind_classifies_parse_failures"),
        "{:?}",
        findings[0].context
    );
    Ok(())
}

#[cfg(feature = "verus_ir")]
#[test]
fn scan_rust_source_exempts_a_ghost_proven_unreachable_arm() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
use verus_builtin_macros::verus;

verus! {

pub fn verify_int_error_kind_classifies_parse_failures(s: &str) -> (result: bool)
    requires
        s@.len() == 0,
    ensures
        result,
{
    match <i32 as std::str::FromStr>::from_str(s) {
        #[cfg(verus_keep_ghost)]
        Ok(_) => unreached(),
        #[cfg(not(verus_keep_ghost))]
        Ok(_) => unreachable!(),
        Err(_) => true,
    }
}

pub fn matches_on_result_with_no_ghost_sibling(x: i32) -> (result: bool)
{
    match x {
        0 => true,
        _ => unreachable!("no ghost sibling backs this one"),
    }
}

}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    assert_eq!(
        findings.len(),
        1,
        "the ghost-proven-unreachable site must be exempt, leaving only the unpaired one: {findings:?}"
    );
    assert_eq!(findings[0].kind, PanicKind::Unreachable);
    assert!(
        findings[0]
            .context
            .ends_with("matches_on_result_with_no_ghost_sibling"),
        "{:?}",
        findings[0].context
    );
    Ok(())
}

#[cfg(feature = "verus_ir")]
#[test]
fn scan_rust_source_exempts_a_verification_leaf_but_not_a_helper_or_a_real_callee()
-> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_verus shape: a `pub fn verify_*` with a real `ensures`
    // clause, called by nothing else in the crate, is itself the checked
    // claim -- its own .expect()/.unwrap() sites are that verification's
    // failure mechanism, not library API surface. Three functions here
    // isolate each condition: `leaf` (ensures + uncalled -> exempt),
    // `has_a_real_caller` (ensures, but called by `caller` -> NOT exempt),
    // and `no_ensures_uncalled` (uncalled, but no ensures -> NOT exempt).
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
use verus_builtin_macros::verus;

verus! {

pub fn leaf(x: i32) -> (result: i32)
    ensures
        result == x,
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

pub fn has_a_real_caller(x: i32) -> (result: i32)
    ensures
        result == x,
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

pub fn caller(x: i32) -> (result: i32)
{
    has_a_real_caller(x)
}

pub fn no_ensures_uncalled(x: i32) -> (result: i32)
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let contexts: Vec<&str> = findings.iter().map(|f| f.context.as_str()).collect();
    assert!(
        !contexts.iter().any(|c| c.ends_with("::leaf")),
        "leaf has ensures and no caller -- must be exempt: {contexts:?}"
    );
    assert!(
        contexts.iter().any(|c| c.ends_with("::has_a_real_caller")),
        "has_a_real_caller has ensures but IS called by caller -- must still be flagged: {contexts:?}"
    );
    assert!(
        contexts
            .iter()
            .any(|c| c.ends_with("::no_ensures_uncalled")),
        "no_ensures_uncalled has no ensures clause -- must still be flagged: {contexts:?}"
    );
    Ok(())
}

#[test]
fn scan_rust_source_finds_compile_error() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(&file, r#"fn bad() { compile_error!("stop"); }"#)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, PanicKind::CompileError);
    Ok(())
}

#[test]
fn error_surface_classifies_library_binary_and_tests() {
    cordial::init_tracing();
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/lib.rs")),
        ErrorSurface::Library
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/main.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/bin/cordial.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("examples/demo.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("tests/panics_etiquette.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/tests/unit.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("benches/hot.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new(
            "/home/erik/tests/myproject/src/lib.rs"
        )),
        ErrorSurface::Library,
        "a home-directory folder named tests must not reclassify library code"
    );
    assert_eq!(
        ErrorSurface::Library.expected_stack(),
        "internal error types"
    );
    assert_eq!(ErrorSurface::Binary.expected_stack(), "miette");
    assert_eq!(ErrorSurface::Test.expected_stack(), "miette");
}

#[test]
fn panics_in_tests_ask_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/boom.rs"),
        "#[test]\nfn boom() { panic!(\"kaboom\"); }\n",
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test abort sites should name miette: {checklist}"
    );
    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("test"),
        "csv should record the test surface: {csv}"
    );
    Ok(())
}

#[test]
fn panics_in_binaries_ask_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        "fn main() { panic!(\"kaboom\"); }\n",
    )
    .into_diagnostic()
    .wrap_err("main")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Binary — surface with miette"),
        "binary abort sites should name miette: {checklist}"
    );
    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("binary"),
        "csv should record the binary surface: {csv}"
    );
    Ok(())
}

#[test]
fn test_expect_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/tmp.rs"),
        "#[test]\nfn tmp() { let _ = tempfile::tempdir().expect(\"tempdir\"); }\n",
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT") && csv.contains("test"),
        "test expect should remain in CSV: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test expect must be a miette action item: {checklist}"
    );
    Ok(())
}

#[test]
fn test_unwrap_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/tmp.rs"),
        "#[test]\nfn tmp() { let _ = Some(1).unwrap(); }\n",
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-UNWRAP") && csv.contains("test"),
        "test unwrap should remain in CSV: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test unwrap must be a miette action item: {checklist}"
    );
    Ok(())
}

#[test]
fn cfg_test_module_in_src_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn ok() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn tmp() { let _ = Some(1).unwrap(); }\n}\n",
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-UNWRAP"),
        "cfg(test) unwrap should be scanned: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "src cfg(test) must be the test surface, not library: {checklist}"
    );
    assert!(
        !checklist.contains("Library — return internal error types"),
        "src cfg(test) must not ask for CordialError: {checklist}"
    );
    Ok(())
}

#[test]
fn library_writeln_expect_is_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn heading(out: &mut String) {\n    writeln!(out, \"# hi\").expect(\"write heading\");\n}\n",
    )
    .into_diagnostic().wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT"),
        "writeln expect should remain in inventory CSV: {csv}"
    );
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Library — return internal error types"),
        "library writeln expect must ask for a typed wrap: {checklist}"
    );
    assert!(
        !checklist.contains("**Open items:** 0"),
        "library writeln expect must be a checklist action item: {checklist}"
    );
    Ok(())
}

#[test]
fn library_tokenstream_parse_expect_is_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"pub fn eq_token() {
    let _ = "==".parse::<proc_macro2::TokenStream>().expect("always-valid");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT"),
        "tokenstream parse expect should remain in inventory CSV: {csv}"
    );
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Library — return internal error types"),
        "library TokenStream parse expect must ask for a typed wrap: {checklist}"
    );
    assert!(
        !checklist.contains("**Open items:** 0"),
        "library TokenStream parse expect must be a checklist action item: {checklist}"
    );
    Ok(())
}

#[test]
fn chained_unwrap_on_one_line_is_one_site() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"fn take(map: &std::collections::HashMap<String, String>) -> String {
    map.get("k").unwrap().as_str().unwrap().to_string()
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    let unwraps = findings
        .iter()
        .filter(|row| row.kind == PanicKind::Unwrap)
        .count();
    assert_eq!(
        unwraps, 1,
        "chained unwraps on one line should be one site: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_err_and_expect_err_are_flagged_the_same_as_their_ok_counterparts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"fn take(res: Result<i32, i32>) -> i32 {
    res.unwrap_err()
}

fn take2(res: Result<i32, i32>) -> i32 {
    res.expect_err("must be an error")
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let unwraps = findings
        .iter()
        .filter(|row| row.kind == PanicKind::Unwrap)
        .count();
    let expects = findings
        .iter()
        .filter(|row| row.kind == PanicKind::Expect)
        .count();
    assert_eq!(unwraps, 1, "{findings:?}");
    assert_eq!(expects, 1, "{findings:?}");
    assert!(
        findings.iter().any(|row| row.snippet == ".unwrap_err()"),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|row| row.snippet == ".expect_err(\"must be an error\")"),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_asserted_against_directly_is_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_kani::ledger_test shape: the whole point of the call
    // is to extract the error value and assert a fact about it, not to
    // discard or propagate a setup failure.
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn validate_rejects_a_negative_amount(res: Result<i32, TransferError>) {
    let error = res.expect_err("negative amount");
    assert_eq!(error, TransferError::NegativeAmount(-1));
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "error is bound then asserted on directly -- must be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_exemption_matches_even_with_a_multiline_receiver() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable main.rs shape: check_method_call computes its own
    // exempt-lookup line from the whole call expression's span, which
    // starts at the *receiver*'s own beginning for a multi-line
    // receiver -- not the .expect_err( call's own line. Both line
    // computations must agree, or the lookup silently misses (confirmed
    // the hard way: this exact fixture's real-world counterpart stayed
    // flagged until error_assertion_binding's own line computation was
    // fixed to match).
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn clap_rejects_a_single_proof_combined_with_a_retry_selector(res: Result<Cli, ClapError>) {
    let error = build_cli([
        "amenable",
        "verify",
        "kani",
    ])
    .expect_err("conflicting selectors must be rejected");

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "error is bound then asserted on directly, receiver just spans several lines first -- \
         must still be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_err_asserted_against_one_hop_later_is_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    // Real assessment_error_chain_test shape: the bound error feeds a
    // further `let`, and THAT is what's asserted on.
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn load_on_malformed_json_preserves_the_real_serde_error_in_the_chain(res: Result<(), AmenableError>) {
    let error = res.unwrap_err();
    let rendered = error.to_string();
    assert!(rendered.contains("line 1"));
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "error flows into rendered one hop later, which IS asserted on -- must be exempt: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_with_no_later_use_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn discards_the_error(res: Result<i32, i32>) {
    let _error = res.expect_err("must fail");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings.len(),
        1,
        "bound but never used again -- not an assertion pattern, must still be flagged: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_err_bound_and_used_but_never_asserted_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn logs_the_error(res: Result<i32, i32>) {
    let error = res.expect_err("must fail");
    println!("{error}");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;
    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings.len(),
        1,
        "used, but never inside a real assert*! -- must still be flagged: {findings:?}"
    );
    Ok(())
}

#[test]
fn nested_parity_workspace_is_skipped_when_scanning_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::create_dir_all(fixture.path().join("tests/parity/workspaces/child/src"))
        .into_diagnostic()
        .wrap_err("nested")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn boom() { panic!(\"parent\"); }\n",
    )
    .into_diagnostic()
    .wrap_err("parent lib")?;
    fs::write(
        fixture
            .path()
            .join("tests/parity/workspaces/child/src/lib.rs"),
        "pub fn boom() { panic!(\"nested fixture\"); }\n",
    )
    .into_diagnostic()
    .wrap_err("nested lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("parent"),
        "parent abort site should scan: {csv}"
    );
    assert!(
        !csv.contains("nested fixture"),
        "tests/parity under the scanned crate is not production source: {csv}"
    );
    Ok(())
}

#[test]
fn unwrap_reachable_from_a_kani_proof_harness_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
struct Channel;

impl Channel {
    pub fn demonstrate_delivery(self, value: i32) -> Token {
        self.send(value).unwrap();
        Token
    }

    fn send(&self, _value: i32) -> Result<(), &'static str> {
        Ok(())
    }
}

struct Token;

#[kani::proof]
fn verify_delivery() {
    let value: i32 = kani::any();
    let channel = Channel;
    let _token = channel.demonstrate_delivery(value);
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "unwrap() inside demonstrate_delivery is reachable from the #[kani::proof] harness \
         that calls it -- Kani checks reachable panics, not Result return values, so this \
         unwrap is the proof's own failure mechanism: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_reachable_from_a_harness_wrapped_in_another_macro_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    // amenable_derive::harness!(cfg_name, CONST_NAME, { item })'s own real
    // shape: syn::visit::Visit never descends into a macro invocation's
    // token stream on its own, so a #[kani::proof] fn written as a wrapper
    // macro's argument is invisible to root detection unless the scanner
    // goes looking for it -- this is the one real amenable_kani case that
    // motivated that lookup (every #[kani::proof] harness in that crate is
    // written this way, not as a bare item).
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
struct Channel;

impl Channel {
    pub fn demonstrate_delivery(self, value: i32) -> Token {
        self.send(value).unwrap();
        Token
    }

    fn send(&self, _value: i32) -> Result<(), &'static str> {
        Ok(())
    }
}

struct Token;

some_crate::harness! {
    kani, VERIFY_DELIVERY_SRC, {
        #[kani::proof]
        fn verify_delivery() {
            let value: i32 = kani::any();
            let channel = Channel;
            let _token = channel.demonstrate_delivery(value);
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "the harness! wrapper macro's braced argument must still be looked into to find the \
         #[kani::proof] fn inside it: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_reachable_only_via_an_assert_eq_argument_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_kani shape: os_windows_model::kani_encode_wide_bmp_char's
    // only real caller passes it as assert_eq!'s own first argument, not as
    // a bare statement -- syn::visit::Visit never descends into ANY macro's
    // token stream on its own (assertion macros included), so this real
    // call edge was silently invisible to the reachability graph before the
    // fix that added exprs_inside_macro's fallback.
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
pub fn encode_bmp(c: char) -> u16 {
    (c as u32).try_into().expect("c is a BMP scalar value by this fn's own precondition")
}

#[kani::proof]
fn verify_encode_bmp() {
    let c: char = kani::any();
    kani::assume((c as u32) < 0x10000);
    assert_eq!(encode_bmp(c), c as u32 as u16, "message");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "encode_bmp's real call site inside assert_eq!'s own arguments must still register a \
         reachability edge: {findings:?}"
    );
    Ok(())
}

#[test]
fn unreachable_nested_under_cfg_kani_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
#[cfg(kani)]
mod proofs {
    fn from_index(index: u8) -> bool {
        match index {
            0 => false,
            1 => true,
            _ => unreachable!("bounded by kani::assume"),
        }
    }

    #[kani::proof]
    fn verify_from_index() {
        let index: u8 = kani::any();
        kani::assume(index <= 1);
        from_index(index);
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings.is_empty(),
        "code nested under #[cfg(kani)] exists only during Kani verification, so its \
         unreachable!() is part of the proof, not a library surface: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_not_reachable_from_any_kani_proof_is_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
pub fn ordinary_helper(value: Option<i32>) -> i32 {
    value.unwrap()
}

#[kani::proof]
fn unrelated_proof() {
    let _ = 1 + 1;
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings.len(),
        1,
        "ordinary_helper is never called by unrelated_proof -- its unwrap() is a real library \
         surface panic, not exempt just because the crate has some #[kani::proof] harness \
         somewhere: {findings:?}"
    );
    assert_eq!(findings[0].kind, PanicKind::Unwrap);
    Ok(())
}

#[test]
fn panic_outside_cfg_kani_is_still_flagged_even_with_a_cfg_not_kani_twin() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(
        &file,
        r#"
fn symbolic_any() -> i32 {
    #[cfg(kani)]
    {
        0
    }

    #[cfg(not(kani))]
    {
        panic!("symbolic construction is only available under cfg(kani)")
    }
}

#[kani::proof]
fn verify_something() {
    let _ = symbolic_any();
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write sample")?;

    let findings = cordial::scan_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings.len(),
        1,
        "symbolic_any is called from a #[kani::proof] harness, but its panic!() lives in the \
         #[cfg(not(kani))] branch -- that code never runs during Kani verification, so it must \
         stay flagged rather than inherit its caller's kani-reachability: {findings:?}"
    );
    assert_eq!(findings[0].kind, PanicKind::Panic);
    Ok(())
}
