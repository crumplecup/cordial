//! A print is a call of the standard library's macro, however it is imported, and
//! never a local macro that shares its name.

use cordial::{IrView, TracingStdioPolicy, tracing_print_findings};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/projected_crate.rs"]
mod projected_crate;

const LIBRARY: &str = include_str!("fixtures/tracing_print_identity/lib.rs.txt");

/// The lines on which a print is reported.
fn reported_lines() -> miette::Result<Vec<u32>> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let inventory = projected_crate::projected_crate(fixture.path())?
        .source_observability()
        .into_diagnostic()?;
    let records = tracing_print_findings(&inventory, &TracingStdioPolicy::default())
        .into_diagnostic()
        .wrap_err("scan")?;
    let mut lines: Vec<u32> = records.iter().map(|record| record.line()).collect();
    lines.sort_unstable();
    Ok(lines)
}

fn line_of(needle: &str) -> u32 {
    LIBRARY
        .lines()
        .position(|line| line.contains(needle))
        .map_or(0, |index| index as u32 + 1)
}

#[test]
fn a_local_macro_named_println_is_not_stdout() -> miette::Result<()> {
    let lines = reported_lines()?;
    assert!(
        !lines.contains(&line_of("a local macro, not stdout")),
        "{lines:?}"
    );
    Ok(())
}

#[test]
fn stdout_is_found_through_an_alias_and_a_qualified_path() -> miette::Result<()> {
    let lines = reported_lines()?;
    assert_eq!(
        lines,
        [
            line_of("stdout through an alias"),
            line_of("stdout, qualified")
        ]
    );
    Ok(())
}
