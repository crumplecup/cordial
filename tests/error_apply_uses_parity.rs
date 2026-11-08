//! Complete located-use evidence agrees across loader frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;

use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_uses_preserve_locations_paths_and_opaque_boundaries() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_uses'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = [
        "struct Cause; enum Error { Source(Cause) } fn use_it(value: Error) -> Error { value }",
        "use crate::{Cause as Renamed, child::{self, Error as _}}; extern crate alloc as _;",
        "struct Unicode; fn café() { let élève = Unicode; let _ = élève; }",
        "struct r#Error; fn r#try(value: r#Error) -> r#Error { value }",
        "struct Ref<'a, T: Trait> { value: &'a T } impl<'a, T: Trait> Ref<'a, T> { fn read(&self) -> &'a T { self.value } }",
        "fn paths<T>() { let _ = <T as Trait>::Assoc::new(); let _ = <T>::new(); let _ = crate::child::Error; }",
        "struct Record { Cause: Cause } fn make(Cause: Cause) { let _ = Record { Cause }; let Record { Cause } = make(); }",
        "mod inline { #[cfg(unix)] struct Inner; fn use_it() { let _ = crate::Cause; } }",
        "fn local() { struct Local; enum Nested { Inner(Local) } let _ = Local; }",
        "#[derive(Debug, Unknown)] struct Cause; #[value = crate::Cause] fn attr() {}",
        "/// Documentation\nstruct Documented; /** block docs */ enum More { Empty }",
        "//! Inner docs\nstruct Documented; mod inline { //! Nested\n struct Inner; }",
        "macro_rules! generate { ($value:ident) => { struct Cause; fn f() { self::call($value); } }; } generate!(Cause, 'a, r#type, _, if, \"Ignored\");",
        "macro_rules! docs { () => { /// generated doc\n struct Cause; } }",
        "#[cfg_attr(unix, derive(Debug))] struct Conditional;",
        "#[other::cfg(unix)] struct Unconditional;",
        "fn labels() { 'outer: loop { break 'outer; } }",
        "pub(crate) struct Visible; pub(in crate::child) struct Restricted;",
        "fn keywords() { let _ = crate::new(); let _ = super::new(); let _ = self::new(); let _ = Self::new(); }",
        "struct Receiver; impl Receiver { fn read(self: self::Receiver) {} }",
        "#[unsafe(no_mangle)] fn attr() {} #[unsafe(cfg(unix))] fn not_cfg() {}",
        "#[value = make!(Cause)] struct AttrMacro;",
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("apply_uses", root);
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
        for field in [
            "source_identifiers",
            "identifiers",
            "declarations",
            "paths",
            "macro_names",
            "has_cfg",
        ] {
            if actual["uses"][field] != expected["uses"][field] {
                differences.push(format!(
                    "{} {field}: native={} compatibility={}",
                    actual["snapshot"]["path"], actual["uses"][field], expected["uses"][field]
                ));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    // Pin the distinction between tokens inside macros and their written path.
    let macros = actual_files
        .iter()
        .find(|file| {
            file["snapshot"]["path"] == root.join("src/case_12.rs").to_string_lossy().as_ref()
        })
        .ok_or_else(|| miette::miette!("macro file"))?;
    assert_eq!(
        macros["uses"]["macro_names"],
        serde_json::json!([
            "Cause", "_", "a", "call", "f", "fn", "ident", "if", "r#type", "self", "struct",
            "value"
        ])
    );
    Ok(())
}
