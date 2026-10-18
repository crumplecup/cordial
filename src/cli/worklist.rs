//! Human-facing worklist commands.

use std::io::{self, Write};

use tracing::instrument;

use super::commands::{ActCtx, Commands};
use super::run::execute_run_plugins_locked;
use crate::worklist::{json_claim, json_error, json_open, json_stats, json_status};
use crate::{
    CordialError, CordialResult, ItemState, SoftStaleness, Validity, WORKLIST_INSTRUCTIONS,
    Worklist, WorklistLock, default_store_home, load_cordial_config, quality_plugins, render_open,
    render_stats,
};

impl Commands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act_worklist(self, ctx: &ActCtx) -> CordialResult<()> {
        let json_command = match &self {
            Self::Open { json: true } => Some("open"),
            Self::Peek { json: true } => Some("peek"),
            Self::Done { json: true, .. } => Some("done"),
            Self::Skip { json: true, .. } => Some("skip"),
            Self::Submit { json: true, .. } => Some("submit"),
            Self::Stats { json: true, .. } => Some("stats"),
            _ => None,
        };
        let result = match self {
            Self::Open { json } => execute_open(ctx, json),
            Self::Peek { json } => execute_peek(ctx, json),
            Self::Done { ids, json } => execute_claim(ctx, &ids, false, None, json),
            Self::Skip { ids, reason, json } => {
                execute_claim(ctx, &ids, true, reason.as_deref(), json)
            }
            Self::Submit {
                deny_open,
                tiers,
                json,
            } => execute_submit(ctx, deny_open, &tiers, json),
            Self::Stats {
                verbose,
                area,
                all,
                json,
            } => execute_stats(ctx, verbose, area.as_deref(), all, json),
            Self::Instructions => {
                write!(io::stdout(), "{WORKLIST_INSTRUCTIONS}").map_err(Into::into)
            }
            Self::Next => Err(CordialError::invariant(
                "Did you mean `cordial done` or `cordial skip`?",
            )),
            _ => Err(CordialError::invariant("not a worklist command")),
        };
        if let (Some(command), Err(error)) = (json_command, &result) {
            let response = json_error(command, ctx.store(), &error.to_string());
            write_json(response)?;
        }
        result
    }
}

#[instrument(level = "trace", skip(value), err(level = "warn"))]
fn write_json(value: serde_json::Value) -> CordialResult<()> {
    writeln!(io::stdout(), "{value}")?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_submit(
    ctx: &ActCtx,
    deny_open: bool,
    tiers: &[String],
    json: bool,
) -> CordialResult<()> {
    let _lock = WorklistLock::exclusive(ctx.store(), "submit", ctx.progress().as_ref())?;
    let Some(previous) = Worklist::read(ctx.store())? else {
        if !json {
            writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
        }
        return Err(CordialError::invariant("No current worklist"));
    };
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    if previous.validity(&config)? != Validity::Current {
        if !json {
            writeln!(
                io::stdout(),
                "List {} is out of date.\n\nNEXT: cordial open",
                previous.record().generation()
            )?;
        }
        return Err(CordialError::invariant("Worklist is out of date"));
    }
    execute_run_plugins_locked(
        ctx.project_root(),
        ctx.store(),
        ctx.crate_name().as_deref(),
        Some(if tiers.is_empty() {
            previous.record().tiers()
        } else {
            tiers
        }),
        None,
        ctx.store_home().clone(),
        ctx.progress().clone(),
        quality_plugins(),
        deny_open,
        true,
        true,
        json,
    )?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_open(ctx: &ActCtx, json: bool) -> CordialResult<()> {
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
                return write_open(ctx, &list, "open", json);
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
        if !json {
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
        }
        execute_run_plugins_locked(
            ctx.project_root(),
            ctx.store(),
            ctx.crate_name().as_deref(),
            None,
            None,
            ctx.store_home().clone(),
            ctx.progress().clone(),
            quality_plugins(),
            false,
            true,
            false,
            false,
        )?;
    }
    let mut list = Worklist::read(ctx.store())?
        .ok_or_else(|| CordialError::invariant("gate did not write a worklist"))?;
    if list.activate() {
        list.write(ctx.store())?;
    }
    write_open(ctx, &list, "open", json)?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_peek(ctx: &ActCtx, json: bool) -> CordialResult<()> {
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    {
        let _lock = WorklistLock::shared(ctx.store(), ctx.progress().as_ref())?;
        let Some(list) = Worklist::read(ctx.store())? else {
            if json {
                write_json(json_status("peek", ctx.store(), None, "no_list"))?;
            } else {
                writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
            }
            return Ok(());
        };
        if list.validity(&config)? != Validity::Current {
            if json {
                write_json(json_status("peek", ctx.store(), Some(&list), "out_of_date"))?;
            } else {
                writeln!(
                    io::stdout(),
                    "List {} is out of date.\n\nNEXT: cordial open",
                    list.record().generation()
                )?;
            }
            return Ok(());
        }
        if list.current().is_some()
            || list
                .items()
                .iter()
                .all(|item| *item.state() != ItemState::Open)
        {
            return write_open(ctx, &list, "peek", json);
        }
    }
    let _lock = WorklistLock::exclusive(ctx.store(), "peek", ctx.progress().as_ref())?;
    let Some(mut list) = Worklist::read(ctx.store())? else {
        if json {
            write_json(json_status("peek", ctx.store(), None, "no_list"))?;
        } else {
            writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
        }
        return Ok(());
    };
    if list.validity(&config)? != Validity::Current {
        if json {
            write_json(json_status("peek", ctx.store(), Some(&list), "out_of_date"))?;
        } else {
            writeln!(
                io::stdout(),
                "List {} is out of date.\n\nNEXT: cordial open",
                list.record().generation()
            )?;
        }
        return Ok(());
    }
    if list.activate() {
        list.write(ctx.store())?;
    }
    write_open(ctx, &list, "peek", json)?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx, ids, reason), err(level = "warn"))]
pub(super) fn execute_claim(
    ctx: &ActCtx,
    ids: &[String],
    skip: bool,
    reason: Option<&str>,
    json: bool,
) -> CordialResult<()> {
    let command = if skip { "skip" } else { "done" };
    let _lock = WorklistLock::exclusive(ctx.store(), command, ctx.progress().as_ref())?;
    let Some(mut list) = Worklist::read(ctx.store())? else {
        if !json {
            writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
        }
        return Err(CordialError::invariant("No current worklist"));
    };
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    if list.validity(&config)? != Validity::Current {
        if !json {
            writeln!(
                io::stdout(),
                "List {} is out of date.\n\nNEXT: cordial open",
                list.record().generation()
            )?;
        }
        return Err(CordialError::invariant("Worklist is out of date"));
    }
    let changes = if skip {
        list.skip(ids, reason)
    } else {
        list.done(ids)
    };
    let changes = match changes {
        Ok(changes) => changes,
        Err(error) => {
            if !json && ids.is_empty() {
                writeln!(io::stdout(), "NEXT: cordial open")?;
            } else if !json {
                writeln!(io::stdout(), "NEXT: cordial stats --verbose --all")?;
            }
            return Err(error);
        }
    };
    if changes.iter().any(|change| change.was() != change.now()) || list.cursor().is_some() {
        list.write(ctx.store())?;
    }
    if json {
        return write_json(json_claim(
            command,
            ctx.store(),
            &list,
            &list.staleness(ctx.project_root()),
            &changes,
        ));
    }
    for change in &changes {
        if change.was() == change.now() {
            writeln!(
                io::stdout(),
                "{} was already {}. Nothing changed.",
                change.id(),
                if skip { "set aside" } else { "checked off" }
            )?;
        } else {
            writeln!(
                io::stdout(),
                "{} {}: {}",
                if skip { "Set aside" } else { "Checked off" },
                change.id(),
                change.item_line()
            )?;
        }
    }
    if skip && changes.iter().any(|change| change.was() != change.now()) {
        writeln!(
            io::stdout(),
            "It stays unchecked, and `cordial submit` brings it back on the next list."
        )?;
    }
    writeln!(io::stdout())?;
    write!(io::stdout(), "{}", render_open(&list))?;
    Ok(())
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_stats(
    ctx: &ActCtx,
    verbose: bool,
    area: Option<&str>,
    all: bool,
    json: bool,
) -> CordialResult<()> {
    let _lock = WorklistLock::shared(ctx.store(), ctx.progress().as_ref())?;
    let Some(list) = Worklist::read(ctx.store())? else {
        if json {
            write_json(json_status("stats", ctx.store(), None, "no_list"))?;
        } else {
            writeln!(io::stdout(), "No list yet.\n\nNEXT: cordial open")?;
        }
        return Ok(());
    };
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let config = load_cordial_config(ctx.project_root(), &home);
    match list.validity(&config)? {
        Validity::Current => {
            let stale = list.staleness(ctx.project_root());
            if json {
                write_json(json_stats(ctx.store(), &list, &stale, verbose, area, all))?;
            } else {
                writeln!(io::stdout(), "{}", staleness_line(&stale))?;
                write!(io::stdout(), "{}", render_stats(&list, verbose, area, all))?;
            }
        }
        reason if json => write_json(json_status(
            "stats",
            ctx.store(),
            Some(&list),
            invalid_reason(reason),
        ))?,
        reason => writeln!(
            io::stdout(),
            "List {} is out of date ({}).\n\nNEXT: cordial open",
            list.record().generation(),
            invalid_reason(reason)
        )?,
    }
    Ok(())
}

const OPEN_STALE_SECS: u64 = 24 * 60 * 60;

#[instrument(level = "trace", skip(ctx, list), err(level = "warn"))]
fn write_open(ctx: &ActCtx, list: &Worklist, command: &str, json: bool) -> CordialResult<()> {
    if json {
        return write_json(json_open(
            command,
            ctx.store(),
            list,
            &list.staleness(ctx.project_root()),
        ));
    }
    let stale = list.staleness(ctx.project_root());
    if *stale.age_secs() >= OPEN_STALE_SECS {
        writeln!(io::stdout(), "{}", staleness_line(&stale))?;
    }
    write!(io::stdout(), "{}", render_open(list))?;
    Ok(())
}

#[instrument(level = "trace", skip(stale))]
fn staleness_line(stale: &SoftStaleness) -> String {
    format!(
        "List age: {}s; {} Rust sources or Cargo manifests changed since build.",
        stale.age_secs(),
        stale.changed_files()
    )
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
