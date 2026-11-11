//! `ret` and `err` follow the resolved return type: whether it can be recorded
//! with `Debug`, and whether any part of it is borrowed, however it is spelled.

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use tracing_apply_crate::{checklist_for, write_apply_fixture};

const SOURCE: &str = include_str!("fixtures/tracing_recording/returns.rs");

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
        ("Shown::new", "new"),
        ("Hidden::try_new", "try_new"),
        ("read_len", "read_len"),
        ("read_name", "read_name"),
        ("read_alias", "read_alias"),
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
fn a_constructor_records_its_return_only_when_the_type_has_debug() -> miette::Result<()> {
    let updated = applied()?;
    assert!(attribute(&updated, "new")?.contains("ret"), "{updated}");
    assert!(
        !attribute(&updated, "try_new")?.contains("ret"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn err_is_withheld_when_any_part_of_the_return_is_borrowed() -> miette::Result<()> {
    let updated = applied()?;
    assert!(
        attribute(&updated, "read_len")?.contains("err"),
        "{updated}"
    );
    assert!(
        !attribute(&updated, "read_name")?.contains("err"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn a_borrow_hidden_behind_a_type_alias_is_still_a_borrow() -> miette::Result<()> {
    let updated = applied()?;
    assert!(
        !attribute(&updated, "read_alias")?.contains("err"),
        "the alias `Name<'_>` is `&str`:\n{updated}"
    );
    Ok(())
}
