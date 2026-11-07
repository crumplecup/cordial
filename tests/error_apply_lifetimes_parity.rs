//! Complete graph-lifetime evidence agrees across frontends.
#![cfg(feature = "internal_error_chain")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_lifetimes_preserve_scopes_ranges_and_constructor_origins() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_lifetimes'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = [
        "struct Error { source: Cause, file: String, line: u32 } enum Kind { Source(Cause), Named { source: Cause }, Empty }",
        "struct Error<'a> { source: &'a Cause, file: &'static str, anchor: &'a () } enum Kind<'a> { Source(&'a Cause), Named { text: &'a str } }",
        "struct Error<'a, T> where T: Trait<'a> { source: &'a T } struct Tuple<'a>(&'a Cause) where Cause: Trait<'a>;",
        "impl Error { fn new<'a>(source: &'a Cause) -> Self { Self { source: source.clone() } } fn view(&self) -> &Cause { &self.source } }",
        "impl<'a> Error<'a> { fn new(source: &'a Cause) -> Self { Self { source } } fn nested<'b>((source, _): (&'b Cause, u32)) -> Self { Self { source: Box::new(source.clone()) } } }",
        "impl Error { fn new<'a>(_: &'a Cause) -> Self { Self { source: other } } fn plain<'a>(source: &'a Cause) -> Self { let value = source; Self { source: value } } }",
        "impl Error { fn make<'a>(source: &'a Cause) -> Self { Self::Source(source.clone()) } fn generic<'a, T>(source: &'a T) -> Self { Self::Source(source) } }",
        "fn make<'a>(source: &'a Cause) -> Error { Error::new(source) } fn wrapped<'a>(source: &'a Cause) -> Result<(), Error> { let result: Result<(), Error> = Err((source,)); result?; Ok(()) }",
        "fn bad<'a>(source: &'a Cause) -> Error { Error::new(other) } fn wrapped<'a>(source: &'a Cause) -> Result<(), Error> { let result: Result<(), Error> = Err((other,)); result?; Ok(()) }",
        "mod nested { struct Error { source: Cause } impl Error { fn new<'a>(source: &'a Cause) -> Self { fn inner<'b>(source: &'b Cause) -> Error { Error::new(source) } Self { source: source.clone() } } } }",
        "impl<'a> Trait<'a> for Error<'a> { type Output = Error<'a>; fn method<'b>(&'a self, source: &'b Cause) -> Error<'b> { Error::new(source) } } trait Trait<'a> { fn make<'b>(source: &'b Cause) -> Error<'b> { Error::new(source) } }",
        "type Alias<'a> = Result<Error<'a>, crate::Error<'a>>; fn usage<T>() { let _ = Error::<'static>::new(); let _ = <T as Trait<'static>>::Error::<'static>::new(); }",
        "type Alias<T> = <T as Trait>::Error; type Qualified = <Error as Trait<Error>>::Output; type Plain = ::Error<'static>;",
        "struct Error { file: &'static str, source: &'static mut Cause, value: Box<Cause>, marker: bool, unit: () } struct Tuple(Box<Cause>, &'static str, &'static ());",
        "struct Error<'a: 'b, 'b> { source: &'a Cause } impl<'a: 'b, 'b> Error<'a, 'b> { fn new<'c: 'a>(source: &'c Cause) -> Self { Self { source } } }",
        "struct Café<'é> { source: &'é Cause } impl Café<'_> { fn new<'é>(élève: &'é Cause) -> Self { Self { source: élève.clone() } } }",
        "fn scope<'a>(source: &'a Cause) -> Error { struct Nested { source: Cause } impl Nested { fn new<'b>(source: &'b Cause) -> Self { Self { source: source.clone() } } } Error::new(source) }",
        "#[opaque(Error<'static>)] struct Error { #[opaque('a)] source: Cause } fn mac<'a>(source: &'a Cause) -> Error { opaque!('a, Error<'a>); Error::new(source) }",
        "impl Box<Error> { fn new<'a>(source: &'a Cause) -> Self { Self::Source(source) } } impl<T> <T as Trait>::Error { fn new<'a>(source: &'a Cause) -> Self { Self::Source(source) } }",
        "fn retained<'a>(source: &'a Cause) -> Error<'a> where Error<'a>: Trait<'a> { 'label: loop { let _: Error<'a>; break 'label; } Error::new(source) }",
        "struct Error { source: fn(Error) -> Error } type Function = dyn Fn(Error) -> Error; unsafe extern \"C\" { fn foreign<'a>(source: &'a Cause) -> Error<'a>; }",
        "impl Error { fn new<'a>(source: &'a Cause) -> Self { Self { source: source.clone(), ..other } } fn no_source<'a>(message: &'a str) -> Self { Self { message: message.to_owned() } } }",
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("apply_lifetimes", root);
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
        if actual["lifetimes"] != expected["lifetimes"] {
            differences.push(format!(
                "{}: native={} compatibility={}",
                actual["snapshot"]["path"], actual["lifetimes"], expected["lifetimes"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    let constructors = actual_files
        .iter()
        .find(|file| {
            file["snapshot"]["path"] == root.join("src/case_3.rs").to_string_lossy().as_ref()
        })
        .ok_or_else(|| miette::miette!("constructors"))?;
    let first = &constructors["lifetimes"]["impls"][0]["methods"][0];
    assert_eq!(first["constructor_anchors"], serde_json::json!(["a"]));
    assert_eq!(
        first["safe_constructor_lifetimes"],
        serde_json::json!(["a"])
    );
    Ok(())
}
