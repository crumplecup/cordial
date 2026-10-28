//! Check execution, result reporting, and worklist scoring under one store lock.

use std::io::{self, Write};

use super::super::commands::ActCtx;
use super::super::gate_args::GateSelectionArgs;
use crate::{
    CordialError, CordialResult, Disposition, Finding, NamedRunFilter, Plugin, RunAll, RunFilter,
    RunOutcome, Session, SessionBuilder, default_store_home,
};
use tracing::instrument;

#[instrument(level = "debug", skip(ctx, selection, plugins), err(level = "warn"))]
pub(in crate::cli) fn execute_run_plugins(
    ctx: &ActCtx,
    selection: &GateSelectionArgs,
    plugins: Vec<&'static dyn Plugin>,
    deny_open: bool,
    write_worklist: bool,
    command: &'static str,
) -> CordialResult<()> {
    let _worklist_lock =
        crate::WorklistLock::exclusive(ctx.store(), command, ctx.progress().as_ref())?;

    execute_run_plugins_locked(
        ctx,
        selection.tiers(),
        selection.etiquette(),
        plugins,
        RunOptions::new(deny_open, write_worklist, false, false),
    )
}

#[derive(derive_new::new)]
pub(in crate::cli) struct RunOptions {
    deny_open: bool,
    write_worklist: bool,
    score_worklist: bool,
    json_submit: bool,
}

/// Run after the caller has acquired the worklist lock.
#[instrument(level = "debug", skip(ctx, plugins, options), err(level = "warn"))]
pub(in crate::cli) fn execute_run_plugins_locked(
    ctx: &ActCtx,
    tiers: Option<&[String]>,
    etiquette: Option<&str>,
    plugins: Vec<&'static dyn Plugin>,
    options: RunOptions,
) -> CordialResult<()> {
    let project_root = ctx.project_root().as_path();
    let store = ctx.store();
    let crate_name = ctx.crate_name().as_deref();
    let store_home = ctx.store_home().clone();
    let progress = ctx.progress().clone();
    let RunOptions {
        deny_open,
        write_worklist,
        score_worklist,
        json_submit,
    } = options;
    #[cfg(not(feature = "quality"))]
    let _ = (write_worklist, score_worklist, json_submit);
    let effective_store_home = store_home.unwrap_or_else(default_store_home);
    let mut builder = SessionBuilder::new(project_root)
        .with_store_root(store.root().clone())
        .with_store_home(effective_store_home.clone())
        .with_progress_sink(progress.clone());
    for plugin in &plugins {
        builder = builder.register_plugin(*plugin);
    }

    let session = builder.build();
    let filter = run_filter(crate_name, tiers, etiquette);
    #[cfg(feature = "quality")]
    let config = crate::load_cordial_config(project_root, &effective_store_home);
    #[cfg(feature = "quality")]
    let tier_selection = crate::TierSelection::resolve(
        &crate::etiquettes_from_plugins(&plugins, &session),
        &config,
        tiers,
    )?;
    #[cfg(feature = "quality")]
    let run_started = std::time::SystemTime::now();
    let outcome = session.run(filter.as_ref())?;
    let summary = print_run_summary(outcome.as_ref())?;
    #[cfg(feature = "quality")]
    if write_worklist {
        let previous = if score_worklist {
            crate::Worklist::read(store)?
        } else {
            None
        };
        let findings = outcome.findings().collect::<Vec<_>>();
        let report = crate::build_quality_report(&findings)?;
        let source = crate::FindingSource::from_findings(
            crate::ChecklistSource::new(store).fresh_since(run_started),
            &findings,
        );
        let selected_tiers = tier_selection.names().iter().cloned().collect();
        let mut list =
            crate::Worklist::build_with_tiers(&report, store, &source, &config, selected_tiers)?;
        let verified: std::collections::BTreeSet<String> = outcome
            .artifacts()
            .map(|artifact| artifact.name().to_string())
            .collect();
        let scored = previous.map(|previous| {
            let scorecard = crate::worklist::score_verified(&previous, &mut list, &verified);
            list.carry_unverified(&previous, &verified);
            if json_submit {
                crate::worklist::json_submit(store, &previous, &list, &scorecard).to_string() + "\n"
            } else {
                crate::render_submit(&previous, &list, &scorecard)
            }
        });
        list.write(store)?;
        if let Some(summary) =
            scored.filter(|_| !json_submit || !deny_open || summary.open_action_items == 0)
        {
            write!(io::stdout(), "{summary}")?;
        }
    }
    progress.notice(format!(
        "Reports available under {}",
        store.findings_dir().display()
    ));
    if deny_open && !outcome.skipped_tiers().is_empty() {
        writeln!(
            io::stderr(),
            "Tiers not run: {}",
            outcome.skipped_tiers().join(", ")
        )?;
    }
    if deny_open && summary.open_action_items > 0 {
        return Err(CordialError::open_findings(summary.open_action_items));
    }
    Ok(())
}

#[instrument(level = "info", fields(crate_name = crate_name))]
fn run_filter(
    crate_name: Option<&str>,
    tiers: Option<&[String]>,
    etiquette: Option<&str>,
) -> RunFilterChoice {
    if crate_name.is_none() && tiers.is_none() && etiquette.is_none() {
        return RunFilterChoice::All(RunAll);
    }
    let mut filter = match etiquette {
        Some(id) => NamedRunFilter::etiquettes([id]),
        None => NamedRunFilter::all_plugins(),
    };
    if let Some(crate_name) = crate_name {
        filter = filter.with_crate(crate_name);
    }
    if let Some(tiers) = tiers {
        filter = filter.with_tiers(tiers);
    }
    RunFilterChoice::Named(filter)
}

enum RunFilterChoice {
    All(RunAll),
    Named(NamedRunFilter),
}

impl RunFilterChoice {
    #[instrument(level = "trace", skip(self))]
    fn as_ref(&self) -> &dyn RunFilter {
        match self {
            Self::All(filter) => filter,
            Self::Named(filter) => filter,
        }
    }
}

#[instrument(level = "debug", skip(outcome), err(level = "warn"))]
fn print_run_summary(outcome: &dyn RunOutcome) -> CordialResult<RunSummary> {
    let findings = outcome.findings().collect::<Vec<_>>();
    let mut open = 0usize;
    let mut exemplar = 0usize;
    let mut suppressed = 0usize;
    for finding in &findings {
        match finding.disposition() {
            Disposition::Open => open += 1,
            Disposition::Exemplar => exemplar += 1,
            Disposition::Suppressed => suppressed += 1,
        }
    }
    let open_action_items = open_action_items(&findings)?;
    let artifacts: Vec<_> = outcome
        .artifacts()
        .map(|artifact| artifact.name())
        .collect();
    tracing::info!(open, exemplar, suppressed, open_action_items, "findings");
    if !artifacts.is_empty() {
        tracing::info!(?artifacts, "artifacts");
    }
    Ok(RunSummary { open_action_items })
}

#[cfg(feature = "quality")]
#[instrument(level = "debug", skip(findings), err(level = "warn"))]
fn open_action_items(findings: &[&dyn Finding]) -> CordialResult<usize> {
    Ok(crate::build_quality_report(findings)?.total_open_items())
}

#[cfg(not(feature = "quality"))]
#[instrument(level = "debug", skip(findings))]
fn open_action_items(findings: &[&dyn Finding]) -> CordialResult<usize> {
    Ok(findings
        .iter()
        .filter(|finding| finding.disposition() == Disposition::Open)
        .count())
}

struct RunSummary {
    open_action_items: usize,
}
