//! Complete getter consumer recipes agree across loader frontends.
#![cfg(feature = "internal_error_chain")]
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn native_getter_consumers_preserve_receivers_scopes_and_macro_locations() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='getter_consumers'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let cases = [
        r#"use derive_getters::Getters;
#[derive(Getters)] struct Failure { file: String }
impl Failure { fn file(&self) -> &str { self.file.as_str() } }
fn read(value: &Failure) { value.file(); value.file; Failure::file(value); }
"#,
        r#"fn read(value: &std::panic::Location<'_>) { value.file(); let loc = std::panic::Location::caller(); loc.file(); (loc).file(); (&loc).file(); }
fn generic<T>(value: T) { value.file(); }
fn constant<const N: usize>(value: &Failure) { value.file(); }
"#,
        r#"fn read(value: &Failure) {
let a = value; a.file(); let mut b = value; b.file(); let ref c = value; c.file();
let (d, e) = pair; d.file(); e.file(); let f: &Other = value; f.file();
let _ = |value: &Other| value.file(); value.file();
{ let value: &Other = other; value.file(); } value.file();
let _ = || { value.file(); };
}
"#,
        r#"fn read(value: &Failure) {
fn make() -> Failure { unknown() } make().file();
fn generic<T>() -> Failure { unknown() } generic().file();
fn constant<const N: usize>() -> Failure { unknown() } constant().file();
{ fn make() -> Other { unknown() } make().file(); } make().file();
let make = unknown; make().file();
}
"#,
        r#"impl Failure {
fn file(&self) -> &str { self.file }
fn read(&self, other: Self, reference: &Self) { self.file(); other.file(); reference.file(); Self::file(self); }
fn nested(&self) { fn local(value: &Self) { value.file(); } self.file(); }
}
impl<T> Generic<T> { fn read(&self) { self.file(); } }
impl Trait for Failure { fn read(&self) { self.file(); } }
trait Defaulted { fn read(&self, value: &Failure) { self.file(); value.file(); } }
"#,
        r#"fn read(value: &Failure) {
mod inner { fn read(value: &Failure) { value.file(); } }
value.file();
}
fn local(value: &Failure) { use crate::Other; value.file(); }
fn matching(value: &Failure) { match value { value => value.file() }; value.file(); }
"#,
        r#"fn read(value: &Failure) { <Failure as Trait>::file(value); Failure::<T>::file(value); ::Failure::file(value); std::panic::Location::file(loc); }
fn typed(value: &Failure<T>, other: <Failure as Trait>::Type) { value.file(); other.file(); }
"#,
        r#"impl Failure { #[cfg(unix)] fn file(&self) {} }
impl Other { #[cfg_attr(unix, tracing::instrument)] fn file() {} }
impl Third { #[cfg_attr(unix, inline)] fn file(&self) {} }
#[derive(derive_getters::Getters)] struct Fourth { #[getter(skip)] file: String }
#[derive(derive_getters::Getters)] struct Fifth { file: String }
"#,
        r#"fn read(value: &Failure) {
format!("{}", value.file()); format!("{}", value.file());
let café = value; std::format!("{}", café.file());
assert!(value.file()); vec![value.file(), value.file()];
format!("{}", format!("{}", value.file()));
unknown!(value.file(), Failure::file(value), (value.file()));
vec![value.file(); 3]; assert!();
}
"#,
        r#"amenable_derive::harness!(One, Two, {
fn read(value: &Failure) { format!("{}", value.file()); }
});
amenable_derive::gallery_harness!(One, Two, {});
harness!(One, Two, { fn read(value: &Failure) { value.file(); } });
"#,
        r#"macro_rules! format { ($e:expr) => { value.file() } }
fn read(value: &Failure) { format!(value.file()); std::format!("{}", value.file()); }
"#,
        r#"use custom::*; fn read(value: &Failure) { format!("{}", value.file()); }
mod std {} fn location(loc: &std::panic::Location<'_>) { loc.file(); }
"#,
        r#"fn read(value: &Failure) { let _ = format!("{}", value.file()); unknown!(value.r#file(), value . file, value :: file); }
"#,
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("getter_consumers", root);
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
        if actual["getter_uses"] != expected["getter_uses"] {
            differences.push(format!(
                "{}: native={} compatibility={}",
                actual["snapshot"]["path"], actual["getter_uses"], expected["getter_uses"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    assert!(actual_files.iter().any(|file| {
        file["getter_uses"]["uses"]
            .as_array()
            .is_some_and(|uses| uses.iter().any(|site| site["kind"] == "Macro"))
    }));
    Ok(())
}
