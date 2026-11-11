//! Getters behind conditional `display` and storage attributes, and ignored error fields.
#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;
use std::fs;
use std::process::Command;

#[path = "support/error_getters_fixture.rs"]
mod error_getters_fixture;

use error_getters_fixture::{ORIGINAL, check, configure, fixture};

#[test]
fn conditional_display_getters_round_trip_in_both_configurations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(&manifest, format!("{cargo}\n[features]\nalternate = []\n")).into_diagnostic()?;
    let original = ORIGINAL.replace(
        "#[display(\"failure at {file}:{line}\")]",
        r#"#[cfg_attr(not(feature = "alternate"), display("failure at {file}:{line}"))]
#[cfg_attr(feature = "alternate", cfg_attr(all(), display("failure at {}:{}", file, line)))]"#,
    );
    let path = root.path().join("src/lib.rs");
    fs::write(&path, &original).into_diagnostic()?;
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
    }
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn conditional_storage_contracts_stay_blocked_when_any_branch_is_unknown() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    for attr in [
        "#[cfg_attr(feature = \"other\", unknown)]",
        "#[cfg_attr(feature = \"other\", cfg(unix))]",
        "#[cfg_attr(feature = \"other\", display(\"{}\", file.capacity()))]",
        "#[cfg_attr(feature = \"other\", display(\"{file}\"), unknown)]",
        "#[cfg_attr(feature = \"other\", cfg_attr(unix, unknown))]",
        "#[cfg_attr(feature = \"other\", derive(unknown::Getters))]",
        "#[cfg_attr(feature = \"other\", derive(derive_getters::Getters))]",
    ] {
        let original =
            ORIGINAL.replace("pub struct Failure", &format!("{attr}\npub struct Failure"));
        fs::write(&path, &original).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{attr}\n{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    }
    Ok(())
}

#[test]
fn ignored_error_field_round_trip_preserves_derive_contract() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = ORIGINAL.replace("file: String,", "#[error(ignore)] file: String,");
    fs::write(&path, &original).into_diagnostic()?;
    check(root.path())?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 1, "{result:?}");
        assert!(
            fs::read_to_string(&path)
                .into_diagnostic()?
                .contains("#[error(ignore)]")
        );
        check(root.path())?;
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}
