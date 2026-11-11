//! A parameter is skipped by `#[instrument]` exactly when `Debug` is not proven
//! for its type in the function's own environment, whatever the type is called.

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use tracing_apply_crate::{checklist_for, write_apply_fixture};

const SOURCE: &str = include_str!("fixtures/tracing_recording/params.rs");

/// The `skip(..)` the applied attribute above `fn name` carries, or none.
fn skipped(updated: &str, name: &str) -> miette::Result<Vec<String>> {
    let lines: Vec<&str> = updated.lines().collect();
    let declaration = lines
        .iter()
        .position(|line| line.contains(&format!("fn {name}")))
        .ok_or_else(|| miette::miette!("no `{name}` in:\n{updated}"))?;
    let attribute = lines[..declaration]
        .iter()
        .rev()
        .take_while(|line| !line.trim().is_empty())
        .find(|line| line.contains("instrument"))
        .ok_or_else(|| miette::miette!("`{name}` was not instrumented:\n{updated}"))?;
    let Some(start) = attribute.find("skip(") else {
        return Ok(Vec::new());
    };
    let rest = &attribute[start + "skip(".len()..];
    let inside = rest.split(')').next().unwrap_or_default();
    Ok(inside
        .split(',')
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect())
}

fn applied() -> miette::Result<String> {
    cordial::init_tracing();
    let names = [
        "derived",
        "collection",
        "bounded",
        "unbounded",
        "opaque",
        "destructured",
        "mixed",
        "borrowed_path",
    ];
    let items: Vec<(&str, u32)> = names
        .iter()
        .map(|name| {
            let line = SOURCE
                .lines()
                .position(|line| line.contains(&format!("fn {name}")))
                .unwrap_or_default();
            (*name, line as u32 + 1)
        })
        .collect();
    let fixture = write_apply_fixture(SOURCE, &checklist_for(&items))?;
    run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    fs::read_to_string(&fixture.src).into_diagnostic()
}

#[test]
fn a_type_with_a_derived_debug_is_recorded() -> miette::Result<()> {
    let updated = applied()?;
    assert_eq!(skipped(&updated, "derived")?, Vec::<String>::new());
    assert_eq!(skipped(&updated, "collection")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_generic_parameter_is_recorded_only_when_its_bound_says_debug() -> miette::Result<()> {
    let updated = applied()?;
    assert_eq!(skipped(&updated, "bounded")?, Vec::<String>::new());
    assert_eq!(skipped(&updated, "unbounded")?, ["value"]);
    Ok(())
}

#[test]
fn a_type_without_debug_is_skipped() -> miette::Result<()> {
    let updated = applied()?;
    assert_eq!(skipped(&updated, "opaque")?, ["value"]);
    assert_eq!(skipped(&updated, "mixed")?, ["hidden"]);
    Ok(())
}

#[test]
fn only_the_unrecordable_binding_of_a_destructured_parameter_is_skipped() -> miette::Result<()> {
    let updated = applied()?;
    assert_eq!(skipped(&updated, "destructured")?, ["right"]);
    Ok(())
}

#[test]
fn borrowed_standard_types_are_recorded() -> miette::Result<()> {
    let updated = applied()?;
    assert_eq!(skipped(&updated, "borrowed_path")?, Vec::<String>::new());
    Ok(())
}
