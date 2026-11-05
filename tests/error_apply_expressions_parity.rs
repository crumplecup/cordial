//! Complete expression and lifetime-origin evidence agrees across frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_expressions_preserve_edits_anchors_and_macro_guards() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_expressions'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = [
        "struct Error { source: Cause, message: String } fn new(source: Cause, message: String) { let _ = Error { source, message, ..other }; let _ = Error { source: source.clone(), message: message.to_owned() }; }",
        "enum Kind { Source(Cause), Other } fn make(value: Cause) { let _ = Kind::Source(value.clone()); let _ = crate::Kind::Source(Box::new(value.clone())); }",
        "struct Parent; impl Parent { fn new(value: Cause) { let _ = Self { source: value }; let _ = Self::Source(value); } fn kind(&self) { self.kind.as_ref(); match self.kind { _ => {} } let _ = other.source; let _ = self.0; } }",
        "impl std::error::Error for Parent { fn source(&self) -> Option<&dyn std::error::Error> { Some(&self.source) } }",
        "impl core::error::Error for Parent { fn source(&self) -> Option<&dyn core::error::Error> { Some(self.source) } }",
        "impl std::error::Error for Parent { fn source(&self) -> Option<&dyn std::error::Error> { Some(self.source.as_ref()) } }",
        "impl Trait for Parent { fn source(&self) { Some(&self.source) } } trait Other { fn source(&self) { Some(&self.source); } }",
        "impl<'a> Parent<'a> { fn new<'b>(source: &'b Cause, (message, context): (&'a str, &'b str)) { let _ = Parent { source: Box::new(source.clone()), message: message.to_string(), context: &context }; let _ = Kind::Source(source); } }",
        "fn new<'a>(value: &'a Cause) { let _ = Error { source: value.clone(), message: other.clone() }; fn nested(value: Cause) { let _ = Error { source: value }; } let _ = Error { source: value }; }",
        "fn patterns(value: Error) { let Error { source, ref message, other: renamed, .. } = value; let _ = (source, message); }",
        "fn macros() { unknown!(Error { source: value }, other.source, other.method()); write!(out, \"{}\", self.source); writeln!(out, \"{}\", self.message); write!(out, \"{}\", self.source.clone()); write!(out, \"{}\", other.source); write!(out, \"{}\", x = self.source); }",
        "macro_rules! build { ($value:expr) => { Self { source: $value } }; } fn macros() { write!(out, b\"{}\", self.source); write!(self.source, \"{}\", self.other); std::write!(out, \"{}\", self.source); write!(out, \"{}\", unknown!(self.source)); write!(out, \"{}\", write!(out, \"{}\", self.source)); }",
        "fn paths<T>() { let _ = <T as Trait>::Error::Source(value); let _ = <T>::Source(value); let _ = Box::extra::new(value.clone()); }",
        "struct Café; impl Café { fn make<'é>(élève: &'é Cause) { let _ = Café { source: élève.clone() }; } }",
        "mod nested { struct Error; impl Error { fn new<'a>(source: &'a Cause) { let _ = Self { source: source.clone() }; } } }",
        "fn constructors<'a>((left, (right, last)): (&'a Cause, (&'a Cause, &'a Cause)), unanchored: Cause) { let _ = Error { left: Box::new(Box::new(left)), right: &right, last: last.clone(), other: (unanchored) }; }",
        "#[value = Error::Source(value.clone())] struct Attribute; #[opaque(Error::Source(value))] struct Opaque;",
        "fn shapes<'a>(value: &'a Cause) { let _ = Error { source: ::Box::new(value.clone()) }; let _ = Error { source: Box::<Cause>::new(value.clone()) }; let _ = Error { source: (value).clone() }; let _ = Error { source: value.clone::<Cause>() }; }",
        "fn rest(value: Error) { let Error { 0: source, .. } = value; let _ = Error { 0: source }; }",
        "impl Box<Parent> { fn new() { let _ = Self { source: other }; } } impl <T as Trait>::Parent { fn new() { let _ = Self { source: other }; } }",
        "fn macros() { opaque!(/// docs\n Error { source: value }); write!(out, r#\"{}\"#, self.source,); }",
        "trait Defaults { fn new<'a>(source: &'a Cause) { let _ = Error { source }; } } unsafe extern \"C\" { fn foreign<'a>(source: &'a Cause); }",
        "impl Parent { fn nested<'a>(source: &'a Cause) { impl Other { fn new<'b>(source: &'b Cause) { let _ = Self { source }; } } let _ = Self { source }; } }",
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("apply_expressions", root);
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
        if actual["expressions"] != expected["expressions"] {
            differences.push(format!(
                "{}: native={} compatibility={}",
                actual["snapshot"]["path"], actual["expressions"], expected["expressions"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    let anchors = actual_files
        .iter()
        .find(|file| {
            file["snapshot"]["path"] == root.join("src/case_7.rs").to_string_lossy().as_ref()
        })
        .ok_or_else(|| miette::miette!("anchors"))?;
    let first = &anchors["expressions"]["events"][0]["Struct"]["fields"];
    assert_eq!(first[0][1]["anchored_lifetimes"], serde_json::json!(["b"]));
    assert_eq!(first[1][1]["anchored_lifetimes"], serde_json::json!(["a"]));
    assert_eq!(first[2][1]["anchored_lifetimes"], serde_json::json!(["b"]));
    Ok(())
}
