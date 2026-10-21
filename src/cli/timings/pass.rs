//! Isolated child process for one timing pass.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use tracing::instrument;

use super::{TimingPass, TimingPassResult};
use crate::cli::commands::ActCtx;
use crate::{
    CordialError, CordialResult, Session, SessionBuilder, all_plugins, default_store_home,
};

struct TimingFilter<'a> {
    included: &'a [String],
    crate_name: Option<&'a str>,
}

impl crate::session::RunFilter for TimingFilter<'_> {
    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> Option<&[String]> {
        Some(self.included)
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> Option<&str> {
        self.crate_name
    }
}

#[instrument(level = "info", skip(ctx))]
pub(super) fn run_child(
    ctx: &ActCtx,
    label: &str,
    included: &[String],
    target: &Path,
    pass: &str,
) -> CordialResult<TimingPassResult> {
    let task = ctx
        .progress()
        .spinner(format!("Measuring {label} ({pass})"));
    let home = ctx.store_home().clone().unwrap_or_else(default_store_home);
    let result_path = target.join(format!("cordial-timings-{pass}.json"));
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--project")
        .arg(ctx.project_root())
        .arg("--store-home")
        .arg(home)
        .arg("timings-pass")
        .arg(label)
        .arg("--output")
        .arg(&result_path)
        .env("CORDIAL_TIMINGS_TARGET_DIR", target)
        .env("CARGO_TARGET_DIR", target)
        .env("CORDIAL_NO_PROGRESS", "1")
        .stdout(Stdio::null());
    for id in included {
        command.arg("--include").arg(id);
    }
    if let Some(name) = ctx.crate_name() {
        command.arg("--crate-name").arg(name);
    }
    let output = command.output()?;
    if !output.status.success() {
        task.finish(format!("Measurement failed for {label} ({pass})"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .find_map(|line| line.split_once("error=").map(|(_, detail)| detail.trim()))
            .unwrap_or("timing pass failed; inspect the project and etiquette configuration");
        return Ok(TimingPassResult::Failed(detail.to_string()));
    }
    let result: TimingPass = serde_json::from_slice(&fs::read(&result_path)?)
        .map_err(|error| CordialError::json_parse(format!("timings-pass {label}"), error))?;
    task.finish(format!(
        "Measured {label} ({pass}) in {:.2}s",
        result.wall_time_nanos as f64 / 1_000_000_000.0
    ));
    Ok(TimingPassResult::Measured(result))
}

/// Hidden process entry point; writes one JSON object to a private file.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
pub(in crate::cli) fn execute_timings_pass(
    ctx: &ActCtx,
    label: &str,
    included: &[String],
    output: &Path,
) -> CordialResult<()> {
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
    let crate_name = ctx.crate_name();
    let filter = TimingFilter {
        included,
        crate_name: crate_name.as_deref(),
    };
    let started = Instant::now();
    let outcome = session.run(&filter)?;
    let pass = TimingPass {
        id: label.to_string(),
        included: included.to_vec(),
        wall_time_nanos: started.elapsed().as_nanos(),
        hooks: outcome.hook_timings().to_vec(),
    };
    fs::write(output, serde_json::to_vec(&pass)?)?;
    Ok(())
}
