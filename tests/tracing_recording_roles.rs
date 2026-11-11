//! A function's role reads what its return type resolves to: `Self` through an
//! alias, and a `bool` anywhere in it, not the way either is written.

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use tracing_apply_crate::{checklist_for, write_apply_fixture};

const SOURCE: &str = include_str!("fixtures/tracing_recording/roles.rs");

/// The attribute applied above the function declared as `fn {name}(`.
fn attribute(updated: &str, name: &str) -> miette::Result<String> {
    let lines: Vec<&str> = updated.lines().collect();
    let declaration = lines
        .iter()
        .position(|line| line.contains(&format!("fn {name}(")))
        .ok_or_else(|| miette::miette!("no `{name}` in:\n{updated}"))?;
    lines[..declaration]
        .iter()
        .rev()
        .take_while(|line| !line.trim().is_empty() && !line.contains('{'))
        .find(|line| line.contains("instrument"))
        .map(|line| (*line).to_owned())
        .ok_or_else(|| miette::miette!("`{name}` was not instrumented:\n{updated}"))
}

fn applied() -> miette::Result<String> {
    cordial::init_tracing();
    let items = [
        ("Shown::from_seed", "from_seed"),
        ("Shown::from_alias", "from_alias"),
        ("Shown::from_checked", "from_checked"),
        ("is_ready", "is_ready"),
        ("has_flag", "has_flag"),
        ("is_total", "is_total"),
    ]
    .map(|(qualified, declared)| {
        let line = SOURCE
            .lines()
            .position(|line| line.contains(&format!("fn {declared}(")))
            .unwrap_or_default();
        (qualified, line as u32 + 1)
    });
    let fixture = write_apply_fixture(SOURCE, &checklist_for(&items))?;
    run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    fs::read_to_string(&fixture.src).into_diagnostic()
}

#[test]
fn a_from_function_returning_self_is_a_constructor_even_through_an_alias() -> miette::Result<()> {
    let updated = applied()?;
    assert!(
        attribute(&updated, "from_seed")?.contains("ret"),
        "{updated}"
    );
    assert!(
        attribute(&updated, "from_alias")?.contains("ret"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn a_from_function_returning_something_else_is_not() -> miette::Result<()> {
    let updated = applied()?;
    assert!(
        !attribute(&updated, "from_checked")?.contains("ret"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn a_predicate_is_a_function_whose_return_contains_a_bool() -> miette::Result<()> {
    let updated = applied()?;
    let trace = "level = \"trace\"";
    assert!(
        attribute(&updated, "is_ready")?.contains(trace),
        "{updated}"
    );
    assert!(
        attribute(&updated, "has_flag")?.contains(trace),
        "{updated}"
    );
    assert!(
        !attribute(&updated, "is_total")?.contains(trace),
        "{updated}"
    );
    Ok(())
}
