//! Complete closed local-lifetime contracts agree across frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_local_lifetimes_preserve_closed_units_and_precise_edits() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='local_lifetimes'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let owned = r#"#[derive(Debug)]
struct Failure { message: String, line: u32 }
impl Failure { fn new<'a>(message: &'a str, line: u32) -> Self { Self { message: message.to_string(), line } } }
impl std::fmt::Display for Failure { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) } }
impl std::error::Error for Failure {}
fn make<'a>(message: &'a str, line: u32) -> Failure { Failure::new(message, line) }
"#;
    let borrowed = owned
        .replace("struct Failure {", "struct Failure<'a> {")
        .replace("message: String", "message: &'a str")
        .replace("impl Failure { fn new<'a>", "impl<'a> Failure<'a> { fn new")
        .replace("for Failure {", "for Failure<'a> {")
        .replace("impl std::", "impl<'a> std::")
        .replace("-> Failure {", "-> Failure<'a> {");
    let cases = vec![
        owned.to_string(),
        borrowed.clone(),
        owned.replace("line: u32", "line: ::u32"),
        owned.replace("message: &'a str", "mut message: &'a str"),
        owned.replace(
            "message: &'a str, line: u32",
            "message: &'a str, mut line: u32",
        ),
        owned.replace("fn new<'a>", "#[inline] fn new<'a>"),
        owned.replace("struct Failure", "/// documented\nstruct Failure"),
        owned.replace("impl Failure", "/// documented\nimpl Failure"),
        owned.replace("fn new<'a>", "/// documented\nfn new<'a>"),
        owned.replace("fn new<'a>", "async fn new<'a>"),
        owned.replace("fn new<'a>", "unsafe fn new<'a>"),
        owned.replace("fn new<'a>", "const fn new<'a>"),
        owned.replace("fn new<'a>", "fn new<'a, T>"),
        owned.replace("fn new<'a>", "fn new<'a: 'static>"),
        owned.replace("message: String", "message: String, context: String"),
        owned.replace("line: u32", "line: crate::Scalar"),
        owned.replace("impl Failure {", "impl Failure { fn other() {}"),
        owned.replace("std::error::Error", "custom::error::Error"),
        owned.replace("impl Failure", "unsafe impl Failure"),
        owned.replace("struct Failure {", "struct Failure<T> {"),
        borrowed.replace("struct Failure<'a>", "struct Failure<'a: 'static>"),
        borrowed.replace("impl<'a> Failure<'a>", "impl<'b> Failure<'b>"),
        borrowed.replace("&'a str", "&'a mut str"),
        owned.replace("fn make<'a>", "pub fn make<'a>"),
        owned.replace("fn make<'a>", "#[inline] fn make<'a>"),
        owned.replace("fn make<'a>", "/// documented\nfn make<'a>"),
        owned.replace("Failure::new(message, line)", "Failure::new(other, line)"),
        owned.replace(
            "Failure::new(message, line)",
            "Failure::new(message, line);",
        ),
        owned.replace(
            "Failure::new(message, line)",
            "return Failure::new(message, line)",
        ),
        owned.replace(
            "Failure::new(message, line)",
            "Failure::new::<T>(message, line)",
        ),
        owned.replace(
            "Failure::new(message, line)",
            "::Failure::new(message, line)",
        ),
        owned.replace("-> Failure {", "-> ::Failure {"),
        owned.replace("std::error::Error", "::std::error::Error"),
        owned
            .replace("message", "mésâge")
            .replace("Failure", "Échec"),
        format!("mod inner {{ {owned} }}"),
        format!("{owned}\nstruct Failure {{ other: String }}"),
        format!(
            "{owned}\nstruct Alpha {{ value: String }} impl Alpha {{ fn new<'b>(value: &'b str) -> Self {{ unknown() }} }}"
        ),
        owned.replace("#[derive(Debug)]", "#[derive(Debug)] #[cfg(unix)]"),
        owned.replace("fn new<'a>(message", "fn new<'a>(ref message"),
        owned.replace("impl Failure {", "impl Failure { #![allow(dead_code)]"),
        owned.replace("impl Failure {", "impl Failure { //! documented\n"),
        owned.replace("-> Self {", "-> Self { #![allow(dead_code)]"),
        owned.replace("-> Self {", "-> Self { //! documented\n"),
        owned.replace("-> Failure {", "-> Failure { #![allow(dead_code)]"),
        owned.replace("-> Failure {", "-> Failure { //! documented\n"),
        owned.replace("#[derive(Debug)]", "#[::derive(Debug)]"),
        owned.replace("#[derive(Debug)]", "#[unsafe(derive(Debug))]"),
        owned.replace("message: String", "message: ::String"),
        owned.replace("-> Self {", "-> ::Self {"),
        owned.replace(
            "Failure::new(message, line)",
            "Failure::new(::message, line)",
        ),
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("local_lifetimes", root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let actual_files = actual["files"]
        .as_array()
        .ok_or_else(|| miette::miette!("files"))?;
    let expected_files = expected["files"]
        .as_array()
        .ok_or_else(|| miette::miette!("files"))?;
    assert_eq!(actual_files.len(), expected_files.len());
    let mut differences = Vec::new();
    for (actual, expected) in actual_files.iter().zip(expected_files) {
        assert_eq!(actual["snapshot"], expected["snapshot"]);
        if actual["local_lifetimes"] != expected["local_lifetimes"] {
            differences.push(format!(
                "{}: native={} compatibility={}",
                actual["snapshot"]["path"], actual["local_lifetimes"], expected["local_lifetimes"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    let populated = actual_files
        .iter()
        .filter(|file| {
            file["local_lifetimes"]["sites"]
                .as_array()
                .is_some_and(|sites| !sites.is_empty())
        })
        .count();
    assert!(
        populated > 5,
        "both valid and rejected contracts must be exercised"
    );
    let first = actual_files
        .iter()
        .find(|file| {
            file["snapshot"]["path"] == root.join("src/case_0.rs").to_string_lossy().as_ref()
        })
        .ok_or_else(|| miette::miette!("first contract"))?;
    assert_eq!(
        first["local_lifetimes"]["sites"][0]["plan"]["returns"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    Ok(())
}
