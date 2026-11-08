//! Loader-owned name and initializer safety agrees across source frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;

use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_safety_preserves_shadowing_boundaries_and_immutable_sources() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='apply_safety'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = [
        ("use std::string::String; use core::panic::Location;", false),
        ("use unknown::String;", true),
        ("use unknown::*;", true),
        ("use unknown::{Nested as Location};", true),
        (
            "use crate::{Cause as Renamed, child::{self, Type as _}};",
            false,
        ),
        ("mod hidden { struct String; use unknown::*; }", false),
        ("mod core {}", true),
        ("fn write() {}", true),
        ("struct String;", true),
        ("enum Location {}", true),
        ("union alloc { value: u8 }", true),
        ("trait std {}", true),
        ("const writeln: u8 = 0;", true),
        ("static str: u8 = 0;", true),
        ("macro_rules! write { () => {} }", true),
        ("extern crate other as core;", true),
        ("extern crate alloc;", true),
        ("struct Generic<String> { value: String }", true),
        ("fn generic<Location>() {}", true),
        ("struct Generic<const String: usize>;", false),
        ("fn outer() { struct Location; }", true),
        ("fn outer() { use std::string::String; }", false),
        ("fn outer() { use unknown::Location; }", true),
        ("type Alias = crate::Cause<u8>;", false),
        ("type Cause = other::Cause;", false),
        ("type Alias<T> = <T as Trait>::Assoc;", false),
        ("type String = Other;", true),
        (
            "struct Plain; impl Plain { fn write() {} type Location = Other; const str: u8 = 0; }",
            false,
        ),
        (
            "trait Plain { fn write(); type Location; const str: u8; }",
            false,
        ),
        ("unsafe extern \"C\" { fn write(); }", false),
        ("macro_rules! plain { () => { struct String; } }", false),
        ("extern crate alloc as _;", false),
    ];
    let mut library = String::new();
    for (index, (source, _)) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    library.push_str("mod strings;\n");
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    std::fs::write(
        root.join("src/strings.rs"),
        r#"
const MESSAGE: &'static str = "message";
static CONTEXT: &'static str = "context";
// Ordinary comments leave the declaration unconditional.
const COMMENTED: &'static str = "ok";
/// Documentation is an attribute in the compatibility frontend.
const DOCUMENTED: &'static str = "no";
#[doc = "explicit"] const ATTRIBUTE: &'static str = "no";
static mut MUTABLE: &'static str = "no";
const BORROW_MUT: &'static mut str = unknown();
const ELIDED: &str = "no";
const ABSOLUTE: &'static ::str = "no";
const PAREN: &'static (str) = "no";
const GENERIC: &'static str<u8> = "no";
const DUPLICATE: &'static str = "no";
static DUPLICATE: &'static str = "no";
#[cfg(unix)] const CONDITIONAL: &'static str = "no";
#[cfg(not(unix))] const CONDITIONAL: &'static str = "no";
mod child { const HIDDEN: &'static str = "no"; }
fn local() { const LOCAL: &'static str = "no"; }
"#,
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("apply_safety", root);
    let baseline = projection::baseline(root, &target)?;
    let expected =
        serde_json::to_value(baseline.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    let mut database = cordial::SourceDatabase::load(root).into_diagnostic()?;
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual =
        serde_json::to_value(native.source_error_apply().into_diagnostic()?).into_diagnostic()?;
    assert_eq!(actual, expected);
    let files = actual["files"]
        .as_array()
        .ok_or_else(|| miette::miette!("files"))?;
    for (index, (source, ambiguous)) in cases.iter().enumerate() {
        let path = root.join(format!("src/case_{index}.rs"));
        let file = files
            .iter()
            .find(|file| file["snapshot"]["path"] == path.to_string_lossy().as_ref())
            .ok_or_else(|| miette::miette!("missing case {index}"))?;
        assert_eq!(file["name_audit"]["ambiguous"], *ambiguous, "{source}");
        if index == 4 {
            assert_eq!(
                file["name_audit"]["aliased_types"],
                serde_json::json!(["Cause", "Type"])
            );
            assert_eq!(
                file["name_audit"]["imported_names"],
                serde_json::json!(["Renamed", "_", "self"])
            );
        }
        if index == 23 {
            assert_eq!(
                file["name_audit"]["aliased_types"],
                serde_json::json!(["Cause"])
            );
        }
    }
    let strings = files
        .iter()
        .find(|file| {
            file["snapshot"]["path"] == root.join("src/strings.rs").to_string_lossy().as_ref()
        })
        .ok_or_else(|| miette::miette!("strings"))?;
    assert_eq!(
        strings["static_strings"],
        serde_json::json!(["COMMENTED", "CONTEXT", "MESSAGE"])
    );
    Ok(())
}
