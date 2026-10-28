//! Record apply claims against a current worklist after source writes succeed.

use std::io::{self, Write};
use std::path::Path;

use crate::{CordialResult, ItemState, StoreLayout, Validity, Worklist, WorklistItem};
use tracing::instrument;

#[instrument(level = "debug", skip(store, in_scope, matches), err(level = "warn"))]
pub(super) fn report(
    project_root: &Path,
    store: &StoreLayout,
    store_home: &Path,
    label: &str,
    in_scope: impl Fn(&WorklistItem) -> bool,
    dry_run: bool,
    matches: impl Fn(&WorklistItem) -> bool,
) -> CordialResult<()> {
    let Some(mut list) = Worklist::read(store)? else {
        writeln!(
            io::stdout(),
            "{label} apply: no current worklist; run `cordial open` to track remaining items."
        )?;
        return Ok(());
    };
    let config = crate::load_cordial_config(project_root, store_home);
    if list.validity(&config)? != Validity::Current {
        writeln!(
            io::stdout(),
            "{label} apply: worklist is out of date; run `cordial open` to track remaining items."
        )?;
        return Ok(());
    }
    let ids: Vec<_> = list
        .items()
        .iter()
        .filter(|item| *item.state() != ItemState::Done)
        .filter(|item| in_scope(item))
        .filter(|item| matches(item))
        .map(|item| item.id().as_str().to_string())
        .collect();
    if !dry_run && !ids.is_empty() {
        list.done(&ids)?;
        list.write(store)?;
    }
    let remaining = list
        .items()
        .iter()
        .filter(|item| in_scope(item))
        .filter(|item| *item.state() == ItemState::Open)
        .count();
    let proposed_open = list
        .items()
        .iter()
        .filter(|item| *item.state() == ItemState::Open)
        .filter(|item| ids.iter().any(|id| id == item.id().as_str()))
        .count();
    writeln!(
        io::stdout(),
        "{label} apply: {} worklist item(s) checked off; {remaining} {} item(s) remain open {}.",
        if dry_run { 0 } else { ids.len() },
        label.to_lowercase(),
        if dry_run {
            "on the current list"
        } else {
            "for manual repair"
        }
    )?;
    if dry_run {
        writeln!(
            io::stdout(),
            "Dry run: {} worklist item(s) would be checked off; {} would remain open for manual repair; no source or worklist changes.",
            ids.len(),
            remaining - proposed_open
        )?;
    }
    Ok(())
}
