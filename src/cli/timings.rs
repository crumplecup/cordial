//! Cold and warm per-etiquette measurements using isolated Cargo targets.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::commands::ActCtx;
use crate::{
    CordialError, CordialResult, HookTiming, NamedRunFilter, Session, SessionBuilder, all_plugins,
    default_store_home, etiquettes_from_plugins, load_cordial_config,
};

#[derive(Debug, Serialize, Deserialize)]
struct TimingPass {
    id: String,
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
    cold: Option<TimingPass>,
    warm: Option<TimingPass>,
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
pub(super) fn execute_timings(ctx: &ActCtx, only: Option<&str>) -> CordialResult<()> {
    #[cfg(feature = "quality")]
    let _lock = crate::WorklistLock::exclusive(ctx.store(), "timings", ctx.progress().as_ref())?;

    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let session = SessionBuilder::new(ctx.project_root())
        .with_store_root(ctx.store().root().clone())
        .with_store_home(home.clone())
        .build();
    let plugins = all_plugins();
    let config = load_cordial_config(ctx.project_root(), &home);
    let mut ids: Vec<String> = etiquettes_from_plugins(&plugins, &session)
        .into_iter()
        .filter(|etiquette| config.etiquette_enabled(etiquette.id()))
        .map(|etiquette| etiquette.id().to_string())
        .collect();
    ids.sort();
    if let Some(id) = only {
        if !ids.iter().any(|candidate| candidate == id) {
            return Err(CordialError::unknown_etiquette(id));
        }
        ids.retain(|candidate| candidate == id);
    }

    let mut entries = Vec::with_capacity(ids.len());
    for id in ids {
        let target = tempfile::tempdir()?;
        let (cold, warm, error) = match run_child(ctx, &id, target.path(), "cold")? {
            TimingPassResult::Measured(cold) => match run_child(ctx, &id, target.path(), "warm")? {
                TimingPassResult::Measured(warm) => (Some(cold), Some(warm), None),
                TimingPassResult::Failed(error) => (Some(cold), None, Some(error)),
            },
            TimingPassResult::Failed(error) => (None, None, Some(error)),
        };
        entries.push(TimingEntry {
            id,
            cold,
            warm,
            error,
        });
    }

    let measured_at_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CordialError::invariant(error.to_string()))?
        .as_secs();
    let report = TimingReport {
        version: 1,
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

#[instrument(level = "info", skip(ctx), fields(id = id))]
fn run_child(ctx: &ActCtx, id: &str, target: &Path, pass: &str) -> CordialResult<TimingPassResult> {
    let task = ctx.progress().spinner(format!("Measuring {id} ({pass})"));
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let result_path = target.join(format!("cordial-timings-{pass}.json"));
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--project")
        .arg(ctx.project_root())
        .arg("--store-home")
        .arg(home)
        .arg("timings-pass")
        .arg(id)
        .arg("--output")
        .arg(&result_path)
        .env("CORDIAL_TIMINGS_TARGET_DIR", target)
        .env("CARGO_TARGET_DIR", target)
        .env("CORDIAL_NO_PROGRESS", "1")
        .stdout(Stdio::null());
    if let Some(name) = ctx.crate_name() {
        command.arg("--crate-name").arg(name);
    }
    let output = command.output()?;
    if !output.status.success() {
        task.finish(format!("Measurement failed for {id} ({pass})"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .find_map(|line| line.split_once("error=").map(|(_, detail)| detail.trim()))
            .unwrap_or("timing pass failed; inspect the project and etiquette configuration");
        return Ok(TimingPassResult::Failed(detail.to_string()));
    }
    let result: TimingPass = serde_json::from_slice(&fs::read(&result_path)?)
        .map_err(|error| CordialError::json_parse(format!("timings-pass {id}"), error))?;
    task.finish(format!(
        "Measured {id} ({pass}) in {:.2}s",
        result.wall_time_nanos as f64 / 1_000_000_000.0
    ));
    Ok(TimingPassResult::Measured(result))
}

/// Hidden process entry point; writes one JSON object to a private file.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(super) fn execute_timings_pass(ctx: &ActCtx, id: &str, output: &Path) -> CordialResult<()> {
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let mut builder = SessionBuilder::new(ctx.project_root())
        .with_store_root(ctx.store().root().clone())
        .with_store_home(home)
        .with_progress_sink(ctx.progress().clone())
        .with_measurement_mode();
    for plugin in all_plugins() {
        builder = builder.register_plugin(plugin);
    }
    let session = builder.build();
    let mut filter = NamedRunFilter::etiquettes([id]);
    if let Some(name) = ctx.crate_name() {
        filter = filter.with_crate(name.clone());
    }
    let started = Instant::now();
    let outcome = session.run(&filter)?;
    let pass = TimingPass {
        id: id.to_string(),
        wall_time_nanos: started.elapsed().as_nanos(),
        hooks: outcome.hook_timings().to_vec(),
    };
    fs::write(output, serde_json::to_vec(&pass)?)?;
    Ok(())
}

#[instrument(level = "trace", skip(report))]
fn render_markdown(report: &TimingReport) -> String {
    let mut body = format!(
        "# Etiquette timings\n\nProject: `{}`  \nMeasured: Unix {}\n\nEach etiquette ran alone. Cold and warm passes used the same isolated Cargo target. Durations include session setup, hooks, and rendering.\n\n| Etiquette | Cold (s) | Warm (s) |\n| --- | ---: | ---: |\n",
        report.project, report.measured_at_unix_secs
    );
    for entry in &report.entries {
        let cold = entry
            .cold
            .as_ref()
            .map(|pass| pass.wall_time_nanos as f64 / 1_000_000_000.0);
        let warm = entry
            .warm
            .as_ref()
            .map(|pass| pass.wall_time_nanos as f64 / 1_000_000_000.0);
        body.push_str(&format!(
            "| `{}` | {} | {} |\n",
            entry.id,
            cold.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
            warm.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
        ));
    }
    body.push_str("\n## Pipeline phases\n\nDurations are inclusive; phase rows overlap with hook and pipeline detail in `timings.json`.\n\n| Etiquette | Phase | Cold (s) | Warm (s) |\n| --- | --- | ---: | ---: |\n");
    for entry in &report.entries {
        let phases: BTreeSet<&str> = entry
            .cold
            .iter()
            .chain(entry.warm.iter())
            .flat_map(|pass| pass.hooks.iter())
            .filter(|hook| hook.stage() == "phase")
            .map(|hook| hook.id().as_str())
            .collect();
        for phase in phases {
            let seconds = |pass: &TimingPass| {
                pass.hooks
                    .iter()
                    .find(|hook| hook.stage() == "phase" && hook.id() == phase)
                    .map(|hook| *hook.elapsed_nanos() as f64 / 1_000_000_000.0)
            };
            let cold = entry.cold.as_ref().and_then(seconds);
            let warm = entry.warm.as_ref().and_then(seconds);
            body.push_str(&format!(
                "| `{}` | `{phase}` | {} | {} |\n",
                entry.id,
                cold.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
                warm.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
            ));
        }
    }
    let unavailable: Vec<&TimingEntry> = report
        .entries
        .iter()
        .filter(|entry| entry.error.is_some())
        .collect();
    if !unavailable.is_empty() {
        body.push_str("\n## Unavailable measurements\n\n");
        for entry in unavailable {
            body.push_str(&format!(
                "- `{}`: {}\n",
                entry.id,
                entry.error.as_deref().unwrap_or_default()
            ));
        }
    }
    body
}
