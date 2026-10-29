#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::{ItemState, Worklist, run_error_file_apply};
use miette::IntoDiagnostic;

const ERROR: &str = r#"use std::panic::Location;
#[derive(Debug, derive_more::Display, derive_more::Error)]
#[display("IO error at {}: {source}", path.display())]
pub struct IoSource {
    source: std::io::Error,
    path: std::path::PathBuf,
    file: String,
    line: u32,
}
impl IoSource {
    #[track_caller]
    fn new() -> Self {
        let loc = Location::caller();
        Self { source: std::io::Error::other("cause"), path: "output".into(),
            file: loc.file().to_string(), line: loc.line() }
    }
}
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum Kind { #[display("{_0}")] Io(IoSource) }
#[derive(Debug, derive_more::Display, derive_more::Error)]
#[display("{kind} at {file}:{line}")]
pub struct Envelope {
    #[error(source)]
    kind: Box<Kind>,
    file: String,
    line: u32,
}
impl Envelope {
    #[track_caller]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self { kind: Box::new(Kind::Io(IoSource::new())),
            file: loc.file().to_string(), line: loc.line() }
    }
}
"#;

#[test]
fn public_derive_errors_round_trip_with_exact_worklist_claims() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(root.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    let error_path = root.path().join("src/error.rs");
    test_fixture(root.path())?;
    for (strategy, claims) in [("borrowed", 2), ("owned", 0)] {
        set_strategy(root.path(), strategy)?;
        run_cli(root.path(), home.path(), false, false)?;
        let before = list(&store)?;
        let source = fs::read(&error_path).into_diagnostic()?;
        let worklist = fs::read(store.root().join("worklist.json")).into_diagnostic()?;
        let preview = run_cli(root.path(), home.path(), true, true)?;
        assert!(
            preview.contains("2 field(s) would be converted"),
            "{preview}"
        );
        assert_eq!(fs::read(&error_path).into_diagnostic()?, source);
        assert_eq!(
            fs::read(store.root().join("worklist.json")).into_diagnostic()?,
            worklist
        );
        let output = run_cli(root.path(), home.path(), true, false)?;
        assert!(output.contains("2 field(s) converted"), "{output}");
        assert!(
            output.contains(&format!("{claims} worklist item(s) checked off")),
            "{output}"
        );
        let after = list(&store)?;
        for original in before.items() {
            let repaired = strategy == "borrowed"
                && original.rule_id().as_deref()
                    == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001")
                && (original.item_line().contains("::file`"));
            let current = after.items().iter().find(|item| item.id() == original.id());
            assert_eq!(
                current.map(|item| item.state()),
                Some(if repaired {
                    &ItemState::Done
                } else {
                    original.state()
                }),
                "{}",
                original.item_line()
            );
        }
        let patched = fs::read_to_string(&error_path).into_diagnostic()?;
        assert_eq!(
            patched
                .matches(if strategy == "borrowed" {
                    "file: &'static str"
                } else {
                    "file: String"
                })
                .count(),
            2
        );
        test_fixture(root.path())?;
        let repeat =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(repeat.converted_fields(), 0, "{:?}", repeat.blockers());
    }
    assert_eq!(fs::read_to_string(&error_path).into_diagnostic()?, ERROR);
    Ok(())
}

#[test]
fn call_site_file_does_not_borrow_the_enclosing_types_lifetime() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let original = r#"#[derive(Debug, derive_more::Display, derive_more::Error)]
#[display("{tag} at {file}:{line}")]
pub struct Envelope<'a> { tag: &'a str, file: String, line: u32 }
impl<'a> Envelope<'a> {
    #[track_caller]
    pub fn new(tag: &'a str) -> Self {
        let loc = std::panic::Location::caller();
        Self { tag, file: loc.file().to_string(), line: loc.line() }
    }
}
"#;
    fs::write(root.path().join("src/error.rs"), original).into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), "pub mod error;\n").into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"#[test]
fn borrows_tag_and_preserves_caller() {
    let tag = String::from("tag");
    let expected = line!() + 1;
    let error = public_error_apply::error::Envelope::new(&tag);
    assert_eq!(error.to_string(), format!("tag at {}:{expected}", file!()));
}
"#,
    )
    .into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        set_strategy(root.path(), strategy)?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 1, "{result:?}");
        if strategy == "borrowed" {
            let patched = fs::read_to_string(root.path().join("src/error.rs")).into_diagnostic()?;
            assert!(patched.contains("file: &'static str"));
            assert!(patched.contains("Envelope<'a>"));
        }
        test_fixture(root.path())?;
    }
    assert_eq!(
        fs::read_to_string(root.path().join("src/error.rs")).into_diagnostic()?,
        original
    );
    Ok(())
}

#[test]
fn conditional_modules_preserve_privacy_during_call_site_round_trip() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(
        &manifest,
        format!("{cargo}\n[features]\ndefault = [\"located\"]\nlocated = []\n"),
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"#[cfg(feature = "located")]
pub mod error;
#[cfg(feature = "located")]
pub use error::Envelope;
#[cfg(feature = "located")]
#[track_caller]
pub fn make() -> Envelope { Envelope::new() }
pub mod unrelated;
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/unrelated.rs"),
        r#"#[cfg(feature = "located")]
use crate::Envelope as Alias;
#[cfg(feature = "located")]
pub fn make() -> Alias { Alias::new() }
pub struct Unrelated {}
macro_rules! construct_unrelated {
    () => { impl Unrelated { pub fn new() -> Self { Self {} } } };
}
construct_unrelated!();
"#,
    )
    .into_diagnostic()?;
    let behavior_path = root.path().join("tests/behavior.rs");
    let behavior = fs::read_to_string(&behavior_path).into_diagnostic()?;
    fs::write(
        behavior_path,
        format!("#![cfg(feature = \"located\")]\n{behavior}"),
    )
    .into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        set_strategy(root.path(), strategy)?;
        let preview =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert_eq!(preview.converted_fields(), 2, "{preview:?}");
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 2, "{applied:?}");
        test_fixture(root.path())?;
        test_fixture_with_args(root.path(), &["--no-default-features"])?;
    }
    assert_eq!(
        fs::read_to_string(root.path().join("src/error.rs")).into_diagnostic()?,
        ERROR
    );
    Ok(())
}

#[test]
fn conditional_descendants_and_remapped_modules_still_block_private_field_edits()
-> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    fs::create_dir_all(root.path().join("src/error")).into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        "#[cfg(feature = \"located\")]\npub mod error;\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/error/child.rs"),
        "fn take() -> String { super::Envelope::new().file }\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/elsewhere.rs"),
        "fn take() -> String { super::Envelope::new().file }\n",
    )
    .into_diagnostic()?;
    for declaration in [
        "#[cfg(feature = \"child\")] mod child;",
        "#[path = \"elsewhere.rs\"] mod child;",
        "#[cfg_attr(feature = \"child\", path = \"elsewhere.rs\")] mod child;",
    ] {
        let original = format!("{ERROR}\n{declaration}\n");
        fs::write(root.path().join("src/error.rs"), &original).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert!(
            !result
                .repairs()
                .iter()
                .any(|repair| repair.context().ends_with("Envelope::file")),
            "{declaration}\n{result:?}"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("src/error.rs")).into_diagnostic()?,
            original
        );
    }
    Ok(())
}

#[test]
fn unsafe_private_field_consumers_and_generated_code_stay_blocked() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let error_path = root.path().join("src/error.rs");
    let cases = [
        ERROR.replace("file: String,", "pub file: String,"),
        ERROR.replace(
            "derive_more::Display,",
            "derive_more::Display, UnknownGetter,",
        ),
        ERROR.replace(
            "#[display(\"{kind} at {file}:{line}\")]",
            "#[display(\"{kind}: {:?}\", file.clone().into_bytes())]",
        ),
        format!("{ERROR}\nimpl Envelope {{ pub fn take(self) -> String {{ self.file }} }}"),
        format!(
            "{ERROR}\nmod child {{ use super::Envelope as Alias; fn take() -> String {{ Alias::new().file }} }}"
        ),
        format!("{ERROR}\nfn make() -> Envelope {{ Envelope::new() }}\nmod child;"),
    ];
    fs::create_dir_all(root.path().join("src/error")).into_diagnostic()?;
    for original in cases {
        if original.ends_with("\nmod child;") {
            // No type name appears here: the declaring module's descendants
            // still need a private-field audit when types are inferred.
            fs::write(
                root.path().join("src/error/child.rs"),
                "fn take() { let error = super::make(); let _ = error.file.into_bytes(); }\n",
            )
            .into_diagnostic()?;
        }
        fs::write(&error_path, &original).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert!(
            !result
                .repairs()
                .iter()
                .any(|repair| repair.context().ends_with("Envelope::file")),
            "{original}\n{result:?}"
        );
        assert_eq!(fs::read_to_string(&error_path).into_diagnostic()?, original);
    }
    Ok(())
}

fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    fs::create_dir_all(root.path().join(".cargo")).into_diagnostic()?;
    fs::write(
        root.path().join(".cargo/config.toml"),
        "[net]\noffline = true\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        r#"[package]
name = "public-error-apply"
version = "0.1.0"
edition = "2024"
[dependencies]
derive_more = { version = "2", features = ["display", "error"] }
[dev-dependencies]
miette = "7"
"#,
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), "/// Error graph.\npub mod error;\nuse error::Envelope;\n#[track_caller]\npub fn make() -> Envelope { Envelope::new() }\n").into_diagnostic()?;
    fs::write(root.path().join("src/error.rs"), ERROR).into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"use std::error::Error;
#[test]
fn preserves_source_and_caller() -> miette::Result<()> {
    let expected = line!() + 1;
    let error = public_error_apply::make();
    assert_eq!(error.to_string(), format!("IO error at output: cause at {}:{expected}", file!()));
    let kind = error.source().ok_or_else(|| miette::miette!("missing Kind source"))?;
    let native = kind.source().ok_or_else(|| miette::miette!("missing native source"))?;
    let foreign = native.source().ok_or_else(|| miette::miette!("missing IO source"))?;
    assert!(foreign.downcast_ref::<std::io::Error>().is_some());
    assert_eq!(foreign.to_string(), "cause");
    Ok(())
}
"#,
    )
    .into_diagnostic()?;
    set_strategy(root.path(), "borrowed")?;
    Ok(root)
}

fn set_strategy(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
}

fn test_fixture(root: &Path) -> miette::Result<()> {
    test_fixture_with_args(root, &[])
}

fn test_fixture_with_args(root: &Path, args: &[&str]) -> miette::Result<()> {
    let output = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
        .args(args)
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn run_cli(root: &Path, home: &Path, apply: bool, dry: bool) -> miette::Result<String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
    command
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(["quality", "--etiquette", "internal_error_chain"]);
    if apply {
        command.arg("--apply");
    }
    if dry {
        command.arg("--dry-run");
    }
    let output = command
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn list(store: &cordial::StoreLayout) -> miette::Result<Worklist> {
    Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing worklist"))
}
