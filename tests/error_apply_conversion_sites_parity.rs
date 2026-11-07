//! Both loaders preserve complete candidate-independent conversion contracts.
#![cfg(feature = "internal_error_chain")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]
#[path = "support/error_apply_projection.rs"]
mod apply_projection;
#[path = "support/source_projection.rs"]
mod projection;
use cordial::{ATTR_SOURCE_ERROR_APPLY, CrateTarget, IrView};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn conversion_contracts_and_executable_sites_agree() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='conversion_contracts'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy='borrowed'\n",
    )
    .into_diagnostic()?;
    let cases = [
        r#"#[derive(Debug)] struct Trouble { file: String }
impl std::error::Error for Trouble {}
impl Trouble { fn new() -> Self { Self { file: file!().to_owned() } } }
fn make() -> Trouble { Trouble::new() }
"#,
        r#"#[derive(Debug)] struct Trouble { file: String, source: Cause, kind: Box<Kind> }
impl core::error::Error for Trouble {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        fn nested() { let _ = self.kind; }
        Some(&self.source)
    }
}
impl From<Cause> for Trouble { fn from(source: Cause) -> Self { Self { source, kind: Box::new(Kind), file: file!().to_owned() } } }
impl From<Trouble> for Other { fn from(e: Trouble) -> Self { todo!() } }
"#,
        r#"struct Trouble { file: String }
fn scopes() -> Result<(), Trouble> {
    leaf()?;
    let _ = || { leaf()?; Ok(()) };
    let _ = || -> Result<(), Other> { leaf()?; Ok(()) };
    let _ = || -> Result<(), Trouble> { leaf()?; Ok(()) };
    let _ = async { leaf()?; Ok(()) };
    let _ = try { leaf()?; };
    fn nested() { leaf()?; }
    Ok(())
}
"#,
        r#"struct Trouble { file: String }
fn constructs(value: Trouble) -> Result<crate::Trouble, Trouble> {
 let Trouble { file, .. } = value;
 let crate::Trouble { file: name, .. } = value;
 let _ = Trouble { file, ..value };
 let _ = crate::Trouble { file: name };
 let _ = Trouble::new::<Vec<Trouble>>();
 let _ = Trouble::new;
 let _ = <Trouble as Factory>::new();
 let _ = value.file;
 Ok(value)
}
"#,
        r#"struct Trouble { file: String }
type Output<T> = std::result::Result<T, Trouble>;
type Direct = Trouble;
use crate::{Trouble as Imported, other::*};
fn macros(f: &mut Formatter) {
 write!(f, "{}", self.file);
 writeln!(f, "{}", self.file.len());
 ::write!(f, "{}", self.file);
 ::writeln!(f, "{}", self.file);
 custom!(self.file);
 custom!(Trouble { file: String::new() });
 custom!(Self { file: String::new() });
}
macro_rules! construct { () => { Trouble { file: String::new() } }; }
macro_rules! self_field { () => { self.file }; }
fn dummy() {
}
"#,
        r#"#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Error)]
struct Trouble { file: String }
#[derive(derive_getters::Getters, derive_more::Display, derive_more::Error)]
#[display("{}", file)]
#[cfg_attr(feature="x", display("{}", file))]
struct Known { file: String }
#[derive(Unknown)] #[custom] struct Other { file: String }
"#,
        r#"use derive_getters::Getters as Access;
use derive_more::Display as Show;
#[derive(Access, Show)] #[display("{}", file)] struct Trouble { file: String }
#[derive(Show)] #[display("{}", file.len())] struct Unstable { file: String }
"#,
        r#"mod nested { struct Trouble { file: String } fn f() -> Trouble { Trouble { file: file!().to_owned() } } }
impl<'a> Trouble<'a> { fn new() -> Self { Self { file: "".to_owned() } } }
"#,
        r#"struct Trouble { file: String }
type Output<Result> = Result<(), Trouble>;
mod std {}
type Qualified = std::result::Result<(), Trouble>;
"#,
        r#"struct Trouble { file: String }
trait Factory { type Value; fn f() -> Trouble { leaf()?; Trouble::new() } }
impl Factory for Trouble { type Value = Trouble; }
unsafe extern "C" { fn foreign() -> Trouble; }
"#,
        r#"/// A documented error.
#[derive(Debug)] struct Trouble { /// File metadata.
file: String }
impl std::error::Error for Trouble {
 /// Source documentation.
 fn source(&self) -> Option<&dyn std::error::Error> { None }
}
"#,
        r#"struct Trouble { file: String }
impl Error for Trouble { fn source(&self) -> Option<&dyn Error> { Some(&self.source) } }
impl custom::Error for Trouble { fn source(&self) -> Option<&dyn custom::Error> { Some(&self.source) } }
fn generic<T: Wrapper<Trouble>>(input: Trouble) -> Result<Trouble, Trouble>
where Trouble: Factory<Trouble>, T: Other<Trouble> { leaf()?; Ok(input) }
impl<T: Factory<Trouble>> Wrapper<Trouble> for Trouble
where Trouble: Factory<Trouble> { fn associated(input: Trouble) -> Trouble { input } }
struct Tuple<T>(Trouble, T) where T: Factory<Trouble>;
trait Factory<T>: Wrapper<Trouble> where Trouble: Wrapper<Trouble> { fn f(value: Trouble); }
type Alias<T> where T: Factory<Trouble> = Result<T, Trouble>;
"#,
    ];
    let mut library = String::new();
    for (index, source) in cases.iter().enumerate() {
        library.push_str(&format!("mod case_{index};\n"));
        std::fs::write(root.join(format!("src/case_{index}.rs")), source).into_diagnostic()?;
    }
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()?;
    let target = CrateTarget::new("conversion_contracts", root);
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
        let a = actual["conversion_sites"]["events"]
            .as_array()
            .ok_or_else(|| miette::miette!("events"))?;
        let b = expected["conversion_sites"]["events"]
            .as_array()
            .ok_or_else(|| miette::miette!("events"))?;
        if a != b {
            let first = a
                .iter()
                .zip(b)
                .position(|(a, b)| a != b)
                .unwrap_or(a.len().min(b.len()));
            differences.push(format!(
                "{} event {first}: native {:?} compatibility {:?}; lengths {} {}",
                actual["snapshot"]["path"],
                a.get(first),
                b.get(first),
                a.len(),
                b.len()
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    assert!(
        actual == expected,
        "{}",
        first_difference("source_error_apply", &actual, &expected)
    );
    let materialized = apply_projection::materialized(native.clone(), root)?;
    let expected_materialized = apply_projection::materialized(baseline, root)?;
    assert!(
        materialized == expected_materialized,
        "{}",
        first_difference("materialized", &materialized, &expected_materialized)
    );
    let mut altered = actual.clone();
    for file in altered["files"]
        .as_array_mut()
        .ok_or_else(|| miette::miette!("files"))?
    {
        file["conversion_sites"]["events"] = serde_json::json!([]);
    }
    let mut mutated = native;
    mutated
        .set_attr(mutated.root(), ATTR_SOURCE_ERROR_APPLY, altered)
        .into_diagnostic()?;
    assert_ne!(
        materialized["units"],
        apply_projection::materialized(mutated, root)?["units"],
        "conversion dispatch must consume IR events"
    );
    Ok(())
}

#[tracing::instrument(level = "trace", skip(path, actual, expected))]
fn first_difference(
    path: &str,
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> String {
    match (actual, expected) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                if a.get(key) != b.get(key) {
                    return first_difference(
                        &format!("{path}.{key}"),
                        &actual[key],
                        &expected[key],
                    );
                }
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            for index in 0..a.len().max(b.len()) {
                if a.get(index) != b.get(index) {
                    return first_difference(
                        &format!("{path}[{index}]"),
                        &actual[index],
                        &expected[index],
                    );
                }
            }
        }
        _ => {}
    }
    format!("{path}: native {actual}, compatibility {expected}")
}
