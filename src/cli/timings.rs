//! Cold and warm per-etiquette measurements using isolated Cargo targets.

use std::fs;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::instrument;

mod marginal;
mod pass;
mod render;

use marginal::measure_marginal;
pub(super) use pass::execute_timings_pass;
use pass::run_child;
use render::render_markdown;

use super::commands::ActCtx;
use crate::{
    CordialError, CordialResult, HookTiming, SessionBuilder, all_plugins, default_store_home,
    etiquettes_from_plugins, load_cordial_config,
};

#[derive(Debug, Serialize, Deserialize)]
struct TimingPass {
    id: String,
    included: Vec<String>,
    wall_time_nanos: u128,
    hooks: Vec<HookTiming>,
}

enum TimingPassResult {
    Measured(TimingPass),
    Failed(String),
}

#[derive(Debug, Serialize)]
struct TimingEntry {
    id: String,
    rules: Vec<String>,
    cold: Option<TimingPass>,
    warm: Option<TimingPass>,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    marginal: Option<MarginalMeasurement>,
}

#[derive(Debug, Serialize)]
struct MarginalMeasurement {
    scope: Vec<String>,
    cold: Option<marginal::MarginalPair>,
    warm: Option<marginal::MarginalPair>,
    /// Standalone minus marginal; shared startup, shared hooks, and interactions.
    cold_overlap_nanos: Option<i128>,
    warm_overlap_nanos: Option<i128>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct TimingReport {
    version: u32,
    project: String,
    measured_at_unix_secs: u64,
    entries: Vec<TimingEntry>,
}

/// Measure each enabled etiquette twice, keeping the same empty target for its warm pass.
#[instrument(level = "debug", skip(ctx))]
pub(super) fn execute_timings(
    ctx: &ActCtx,
    only: Option<&str>,
    marginal: bool,
    scope: &[String],
) -> CordialResult<()> {
    #[cfg(feature = "quality")]
    let _lock = crate::WorklistLock::exclusive(ctx.store(), "timings", ctx.progress().as_ref())?;

    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let session = SessionBuilder::new(ctx.project_root())
        .with_store_root(ctx.store().root().clone())
        .with_store_home(home.clone())
        .build();
    let plugins = all_plugins();
    let config = load_cordial_config(ctx.project_root(), &home);
    let available = etiquettes_from_plugins(&plugins, &session);
    let mut ids: Vec<String> = available
        .iter()
        .filter(|etiquette| config.etiquette_enabled(etiquette.id()))
        .map(|etiquette| etiquette.id().to_string())
        .collect();
    ids.sort();
    for id in scope {
        if !ids.iter().any(|candidate| candidate == id) {
            return Err(CordialError::unknown_etiquette(id));
        }
    }
    if !scope.is_empty() {
        ids.retain(|id| scope.contains(id));
    }
    if let Some(id) = only
        && !ids.iter().any(|candidate| candidate == id)
    {
        return Err(CordialError::unknown_etiquette(id));
    }

    let selected: Vec<String> = match only {
        Some(id) => ids
            .iter()
            .filter(|candidate| candidate.as_str() == id)
            .cloned()
            .collect(),
        None => ids.clone(),
    };
    let mut entries = Vec::with_capacity(selected.len());
    for id in selected {
        let target = tempfile::tempdir()?;
        let standalone = [id.clone()];
        let (cold, warm, error) = match run_child(ctx, &id, &standalone, target.path(), "cold")? {
            TimingPassResult::Measured(cold) => {
                match run_child(ctx, &id, &standalone, target.path(), "warm")? {
                    TimingPassResult::Measured(warm) => (Some(cold), Some(warm), None),
                    TimingPassResult::Failed(error) => (Some(cold), None, Some(error)),
                }
            }
            TimingPassResult::Failed(error) => (None, None, Some(error)),
        };
        let rules = available
            .iter()
            .find(|etiquette| etiquette.id() == id)
            .map(|etiquette| {
                etiquette
                    .explain()
                    .rules()
                    .iter()
                    .map(|rule| rule.id().clone())
                    .collect()
            })
            .unwrap_or_default();
        let marginal = if marginal {
            let mut measured = measure_marginal(ctx, &id, &ids)?;
            measured.cold_overlap_nanos = cold
                .as_ref()
                .zip(measured.cold.as_ref())
                .map(|(alone, pair)| alone.wall_time_nanos as i128 - *pair.wall_delta_nanos());
            measured.warm_overlap_nanos = warm
                .as_ref()
                .zip(measured.warm.as_ref())
                .map(|(alone, pair)| alone.wall_time_nanos as i128 - *pair.wall_delta_nanos());
            Some(measured)
        } else {
            None
        };
        entries.push(TimingEntry {
            id,
            rules,
            cold,
            warm,
            error,
            marginal,
        });
    }

    let measured_at_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CordialError::invariant(error.to_string()))?
        .as_secs();
    let report = TimingReport {
        version: if marginal { 2 } else { 1 },
        project: ctx.project_root().display().to_string(),
        measured_at_unix_secs,
        entries,
    };
    fs::create_dir_all(ctx.store().root())?;
    fs::write(
        ctx.store().root().join("timings.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    fs::write(
        ctx.store().root().join("timings.md"),
        render_markdown(&report),
    )?;
    writeln!(
        io::stdout(),
        "Measured {} etiquette(s): {} and {}",
        report.entries.len(),
        ctx.store().root().join("timings.md").display(),
        ctx.store().root().join("timings.json").display()
    )?;
    Ok(())
}
