//! `cordial perf`: measure cordial's own cost and compare runs.

use clap::Subcommand;
use std::io::{self, Write};
use std::path::PathBuf;
use tracing::instrument;

use crate::CordialResult;
use crate::perf::{
    NextestRequest, PerfStore, RunRecord, TimingRequest, measure, measure_from, render_timing,
};
use crate::{CordialError, load_cordial_config};

use super::commands::ActCtx;

/// Subcommands of `cordial perf`.
#[derive(Subcommand)]
pub enum PerfCommands {
    /// Measure the tests themselves.
    Tests {
        /// What to measure about the tests.
        #[command(subcommand)]
        command: PerfTestsCommands,
    },
    /// List the run records kept in this project's store, oldest first.
    List {
        /// Print the records as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

/// Subcommands of `cordial perf tests`.
#[derive(Subcommand)]
pub enum PerfTestsCommands {
    /// Run a set of tests one at a time and rank them by how long they take.
    Timing {
        /// A set named under `[perf.tests]` in cordial.toml, or a label for `--filter`.
        name: String,
        /// A nextest filterset to run instead of the named set.
        #[arg(long, short = 'E')]
        filter: Option<String>,
        /// Run the set this many times; each test's median is reported.
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// How many of the slowest tests to list.
        #[arg(long, default_value_t = 25)]
        top: usize,
        /// Cargo features to build the tests with.
        #[arg(long, default_value = "full")]
        features: String,
        /// Build without the default features, as `just test-slow` does.
        #[arg(long)]
        no_default_features: bool,
        /// Test threads. One keeps tests from contending, which is what timings
        /// need; more finishes a full overview sooner but less exactly.
        #[arg(long, default_value_t = 1)]
        threads: u32,
        /// Build the record from raw event logs a run left under `perf/raw`
        /// (one file per repeat) instead of running the tests.
        #[arg(long, num_args = 1.., value_name = "FILE")]
        from: Vec<PathBuf>,
    },
}

impl PerfCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::List { json } => list(ctx, json),
            Self::Tests { command } => command.act(ctx),
        }
    }
}

impl PerfTestsCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::Timing {
                name,
                filter,
                repeat,
                top,
                features,
                no_default_features,
                threads,
                from,
            } => timing(
                ctx,
                &name,
                TimingOptions {
                    filter,
                    repeat,
                    top,
                    features,
                    no_default_features,
                    threads,
                    from,
                },
            ),
        }
    }
}

/// One table row: id, subject, state, commit, machine.
#[instrument(level = "trace", skip(record))]
fn row(record: &RunRecord) -> String {
    let sha: String = record.identity().sha().chars().take(8).collect();
    let dirty = if record.identity().dirty() { "+" } else { "" };
    format!(
        "{}  {}  {}  {sha}{dirty}  {}",
        record.id(),
        record.subject(),
        record
            .state()
            .map_or_else(|| "-".to_owned(), |state| state.to_string()),
        record.environment().machine()
    )
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn list(ctx: &ActCtx, json: bool) -> CordialResult<()> {
    let records = PerfStore::new(ctx.store()).list()?;
    let mut output = io::stdout().lock();
    if json {
        writeln!(output, "{}", serde_json::to_string_pretty(&records)?)?;
    } else if records.is_empty() {
        writeln!(output, "No performance records yet.")?;
    } else {
        writeln!(output, "id  subject  state  commit  machine")?;
        for record in &records {
            writeln!(output, "{}", row(record))?;
        }
    }
    Ok(())
}

/// The options of one `perf tests timing` invocation.
struct TimingOptions {
    filter: Option<String>,
    repeat: u32,
    top: usize,
    features: String,
    no_default_features: bool,
    threads: u32,
    from: Vec<PathBuf>,
}

/// The nextest filterset for `name`: the one given, or the set the project names.
#[instrument(level = "debug", skip(ctx, given), err(level = "warn"))]
fn filter_of(ctx: &ActCtx, name: &str, given: Option<String>) -> CordialResult<String> {
    if let Some(filter) = given {
        return Ok(filter);
    }
    let store_home = ctx
        .store_home()
        .clone()
        .unwrap_or_else(crate::default_store_home);
    load_cordial_config(ctx.project_root(), &store_home)
        .perf()
        .tests()
        .get(name)
        .map(|set| set.filter().clone())
        .ok_or_else(|| {
            CordialError::perf(
                ctx.project_root().join("cordial.toml"),
                format!("no test set `{name}` under [perf.tests]; pass --filter"),
            )
        })
}

/// Run the named set (or read its raw logs), save its record, and print the ranking.
#[instrument(level = "debug", skip(ctx, options), err(level = "warn"))]
fn timing(ctx: &ActCtx, name: &str, options: TimingOptions) -> CordialResult<()> {
    let store = PerfStore::new(ctx.store());
    let record = if options.from.is_empty() {
        let filter = filter_of(ctx, name, options.filter)?;
        let request = TimingRequest::new(
            name.to_owned(),
            NextestRequest::new(
                filter,
                options.features,
                options.no_default_features,
                options.threads,
            ),
            options.repeat,
        );
        measure(ctx.project_root(), &request, &store.raw_dir())?
    } else {
        measure_from(ctx.project_root(), name, options.threads, &options.from)?
    };
    let path = store.save(&record)?;
    let mut output = io::stdout().lock();
    write!(output, "{}", render_timing(&record, options.top))?;
    writeln!(output, "\nSaved {}", path.display())?;
    Ok(())
}
