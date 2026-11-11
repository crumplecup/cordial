//! Clap types and error types that live only in `main` are an island.

use cordial::CliLayoutId;
use miette::WrapErr;

#[path = "support/cli_layout_fixture.rs"]
mod cli_layout_fixture;

use cli_layout_fixture::{cli_layout_records, write_cli_crate};
#[test]
fn clap_types_only_in_main_are_an_island() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        "pub struct Lib;\n",
        r#"
use clap::Parser;

#[derive(Parser)]
struct Cli {
    name: String,
}

fn main() {}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Island001 && finding.context().contains("Cli")
        }),
        "Parser in main.rs must be CLI-ISLAND: {:?}",
        findings
    );
    Ok(())
}

#[test]
fn bin_only_error_type_is_an_island() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        "pub struct Lib;\n",
        r#"
struct BinaryError;

impl std::error::Error for BinaryError {}
impl std::fmt::Display for BinaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bin")
    }
}

fn main() {}
"#,
    )?;
    let findings = cli_layout_records(fixture.path(), "fixture").wrap_err("scan")?;
    assert!(
        findings.iter().any(|finding| {
            finding.rule_id() == CliLayoutId::Island001 && finding.context().contains("BinaryError")
        }),
        "Error type in main.rs must be CLI-ISLAND: {:?}",
        findings
    );
    Ok(())
}
