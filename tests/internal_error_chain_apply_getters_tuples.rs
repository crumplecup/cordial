//! Tuple getters: origins that are conditional, complementary, incomplete, or escaped.
#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;
use std::fs;
use std::process::Command;

#[path = "support/error_getters_fixture.rs"]
mod error_getters_fixture;

use error_getters_fixture::{ORIGINAL, check, configure, fixture};

const CONDITIONAL_TUPLE: &str = r#"use derive_getters::Getters;
use derive_more::{Display, Error};
/// A located failure.
#[derive(Debug, Display, Error, Getters)]
#[display("failure at {file}:{line}")]
pub struct Failure {
    /// Source file.
    #[cfg(not(kani))]
    file: String,
    #[cfg(not(kani))]
    line: u32,
}
impl Failure {
    #[track_caller]
    pub fn new() -> Self {
        #[cfg(not(kani))]
        let (line, file) = {
            let loc = std::panic::Location::caller();
            (loc.line(), loc.file().to_string())
        };
        Self {
            #[cfg(not(kani))]
            file,
            #[cfg(not(kani))]
            line,
        }
    }
}
"#;

#[test]
fn conditional_tuple_getter_round_trip_compiles_and_restores_source() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    fs::write(&path, CONDITIONAL_TUPLE).into_diagnostic()?;
    check(root.path())?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let preview =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert_eq!(preview.converted_fields(), 1, "{preview:?}");
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 1, "{applied:?}");
        let patched = fs::read_to_string(&path).into_diagnostic()?;
        if strategy == "borrowed" {
            assert!(patched.contains("#[cfg(not(kani))]\ntype CordialFailureFileStorage"));
            assert!(patched.contains("(loc.line(), loc.file())"));
            assert!(patched.contains("#[getter(copy)]"));
        }
        check(root.path())?;
    }
    assert_eq!(
        fs::read_to_string(&path).into_diagnostic()?,
        CONDITIONAL_TUPLE
    );
    Ok(())
}

#[test]
fn complementary_tuple_origins_round_trip_in_both_configurations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(&manifest, format!("{cargo}\n[features]\nalternate = []\n")).into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    let original = complementary_tuples();
    fs::write(&path, &original).into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"#[test]
fn conditional_caller_contract() {
    let expected = line!() + 1;
    let error = getter_apply::Failure::new();
    if cfg!(feature = "alternate") {
        assert_eq!(*error.line(), 0);
        assert_eq!(error.file(), "");
        assert_eq!(error.to_string(), "failure at :0");
    } else {
        assert_eq!(*error.line(), expected);
        assert_eq!(error.file(), file!());
        assert_eq!(error.to_string(), format!("failure at {}:{expected}", file!()));
    }
}
"#,
    )
    .into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let before = fs::read(&path).into_diagnostic()?;
        let preview =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert_eq!(preview.converted_fields(), 1, "{preview:?}");
        assert_eq!(fs::read(&path).into_diagnostic()?, before);
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 1, "{applied:?}");
        let patched = fs::read_to_string(&path).into_diagnostic()?;
        if strategy == "borrowed" {
            assert!(patched.contains("(0, \"\")"));
            assert!(patched.contains("(loc.line(), loc.file())"));
        }
        check(root.path())?;
        let output = Command::new("cargo")
            .args(["test", "--offline", "--quiet", "--features", "alternate"])
            .current_dir(root.path())
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let repeat =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(repeat.converted_fields(), 0, "{repeat:?}");
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn incomplete_or_escaped_tuple_origins_stay_unchanged() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = complementary_tuples();
    for unsupported in [
        original.replace("not(feature = \"alternate\")", "feature = \"another\""),
        original.replace("not(feature = \"alternate\")", "feature = \"alternate\""),
        original.replace("(0, String::new())", "(0, runtime_file())"),
        original.replace("(0, String::new())", "(1, String::new())"),
        original.replace(
            "Self { file, line }",
            "let _escaped = &file; Self { file, line }",
        ),
        original.replace(
            "Self { file, line }",
            "let file = String::new(); Self { file, line }",
        ),
        original.replace("let (line, file)", "let (line, mut file)"),
        original.replace(
            "#[cfg(feature = \"alternate\")]",
            "#[cfg_attr(feature = \"alternate\", cfg(unix))]",
        ),
    ] {
        fs::write(&path, &unsupported).into_diagnostic()?;
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 0, "{unsupported}\n{applied:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, unsupported);
    }
    Ok(())
}

fn complementary_tuples() -> String {
    ORIGINAL.replace(
        "let loc = std::panic::Location::caller();\n        Self { file: loc.file().to_string(), line: loc.line() }",
        r#"#[cfg(feature = "alternate")]
        let (line, file) = (0, String::new());
        #[cfg(not(feature = "alternate"))]
        let (line, file) = {
            let loc = std::panic::Location::caller();
            (loc.line(), loc.file().to_string())
        };
        Self { file, line }"#,
    )
}
