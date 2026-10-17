//! Human-facing worklist commands.

use std::io::{self, Write};

use tracing::instrument;

use super::commands::ActCtx;
use super::run::execute_run_plugins_locked;
use crate::{
    CordialError, CordialResult, ItemState, Validity, Worklist, WorklistLock, all_plugins,
    default_store_home, load_cordial_config, render_open, render_stats,
};

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_open(ctx: &ActCtx) -> CordialResult<()> {
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    {
        let _lock = WorklistLock::shared(ctx.store(), ctx.progress().as_ref())?;
        if let Some(list) = Worklist::read(ctx.store())? {
            if list.validity(&config)? == Validity::Current
                && (list.current().is_some()
                    || list
                        .items()
                        .iter()
                        .all(|item| *item.state() != ItemState::Open))
            {
                return write!(io::stdout(), "{}", render_open(&list)).map_err(Into::into);
            }
        }
    }

    // Another gate or open may have written a list while this command waited.
    let _lock = WorklistLock::exclusive(ctx.store(), "open", ctx.progress().as_ref())?;
    let existing = Worklist::read(ctx.store())?;
    let validity = existing
        .as_ref()
        .map(|list| list.validity(&config))
        .transpose()?;
    if validity != Some(Validity::Current) {
        match (&existing, validity) {
            (None, _) => writeln!(io::stdout(), "No list yet. Building one now.")?,
            (Some(list), Some(reason)) => {
                let checked = list
                    .items()
                    .iter()
                    .filter(|item| *item.state() == ItemState::Done)
                    .count();
                writeln!(
                    io::stdout(),
                    "List {} is out of date ({}). Rebuilding; {checked} checked-off item(s) will be discarded.",
                    list.record().generation(),
                    invalid_reason(reason)
                )?;
            }
            _ => {
                return Err(CordialError::invariant(
                    "worklist validity state is incomplete",
                ));
            }
        }
        execute_run_plugins_locked(
            ctx.project_root(),
            ctx.store(),
            ctx.crate_name().as_deref(),
            ctx.store_home().clone(),
            ctx.progress().clone(),
            all_plugins(),
            false,
            true,
        )?;
    }
    let mut list = Worklist::read(ctx.store())?
        .ok_or_else(|| CordialError::invariant("gate did not write a worklist"))?;
    if list.activate() {
        list.write(ctx.store())?;
    }
    write!(io::stdout(), "{}", render_open(&list))?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_stats(
    ctx: &ActCtx,
    verbose: bool,
    area: Option<&str>,
    all: bool,
) -> CordialResult<()> {
    let _lock = WorklistLock::shared(ctx.store(), ctx.progress().as_ref())?;
    let Some(list) = Worklist::read(ctx.store())? else {
        writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
        return Ok(());
    };
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    match list.validity(&config)? {
        Validity::Current => write!(io::stdout(), "{}", render_stats(&list, verbose, area, all))?,
        reason => writeln!(
            io::stdout(),
            "List {} is out of date ({}).\n\nNEXT: cordial open",
            list.record().generation(),
            invalid_reason(reason)
        )?,
    }
    Ok(())
}

#[instrument(level = "debug", skip(reason))]
fn invalid_reason(reason: Validity) -> &'static str {
    match reason {
        Validity::Current => "current",
        Validity::SchemaChanged => "schema changed",
        Validity::CordialChanged => "cordial version changed",
        Validity::ConfigChanged => "configuration changed",
    }
}
