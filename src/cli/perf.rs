//! `cordial perf`: measure cordial's own cost and compare runs.

use clap::Subcommand;
use std::io::Write;
use std::path::PathBuf;
use tracing::instrument;

use crate::CordialResult;
use crate::perf::{
    DEFAULT_TOLERANCE, NextestRequest, PerfStore, RunRecord, TimingRequest, Verdict,
    WorkflowRequestBuilder, compare, measure, measure_from, measure_workflow, profiled,
    read_profiles, render_comparison, render_profile, render_stages, render_timing,
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
    /// Run cordial on this project cold, warm and after an edit, and record
    /// where each run's time went.
    Workflow {
        /// A workflow named under `[perf.workflows]` in cordial.toml.
        #[arg(default_value = "cordial")]
        name: String,
        /// Runs per state; each stage's median is reported.
        #[arg(long, default_value_t = 3)]
        repeat: u32,
        /// How many of the slowest stages to list per state.
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// The cordial executable to measure (default: this one).
        #[arg(long)]
        program: Option<PathBuf>,
    },
    /// Compare a record against its baseline (or any other record) and say what
    /// moved: exact counters, timings beyond noise and tolerance, changed inputs.
    Compare {
        /// The earlier record (id or unique prefix). Alone, it is the candidate and
        /// is compared with its subject's baseline. Absent, every baselined
        /// subject's newest record is compared with its baseline.
        first: Option<String>,
        /// The later record, when comparing two records directly.
        second: Option<String>,
        /// How far a timing must move, as a share of its earlier median.
        #[arg(long, default_value_t = DEFAULT_TOLERANCE)]
        tolerance: f64,
        /// How many rows of each list to print.
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Exit with an error when any comparison regressed.
        #[arg(long)]
        strict: bool,
    },
    /// Mark a record as the baseline for its subject and state.
    Baseline {
        /// The record (id or unique prefix).
        record: String,
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
    /// Run a set of tests with profiling on and say where their time went:
    /// setup against work, the slowest functions, the tests with most setup.
    Profile {
        /// A set named under `[perf.tests]` in cordial.toml, or a label for `--filter`.
        name: String,
        /// A nextest filterset to run instead of the named set.
        #[arg(long, short = 'E')]
        filter: Option<String>,
        /// How many rows of each list to print.
        #[arg(long, default_value_t = 25)]
        top: usize,
        /// Cargo features; profiling needs `hotpath`.
        #[arg(long, default_value = "full,hotpath")]
        features: String,
        /// Build without the default features, as `just test-slow` does.
        #[arg(long)]
        no_default_features: bool,
        /// Test threads; one keeps the profiled tests from contending.
        #[arg(long, default_value_t = 1)]
        threads: u32,
    },
}

impl PerfCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::List { json } => list(ctx, json),
            Self::Baseline { record } => baseline(ctx, &record),
            Self::Compare {
                first,
                second,
                tolerance,
                top,
                strict,
            } => compare_records(ctx, first, second, (tolerance, top), strict),
            Self::Workflow {
                name,
                repeat,
                top,
                program,
            } => workflow(ctx, &name, repeat, top, program),
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
            Self::Profile {
                name,
                filter,
                top,
                features,
                no_default_features,
                threads,
            } => profile(
                ctx,
                &name,
                TimingOptions {
                    filter,
                    repeat: 1,
                    top,
                    features,
                    no_default_features,
                    threads,
                    from: Vec::new(),
                },
            ),
        }
    }
}

/// One table row: id, subject, state, commit, machine.
#[instrument(level = "trace", skip(record))]
fn row(record: &RunRecord, baseline: bool) -> String {
    let sha: String = record.identity().sha().chars().take(8).collect();
    let dirty = if record.identity().dirty() { "+" } else { "" };
    let mark = if baseline { "  [baseline]" } else { "" };
    format!(
        "{}  {}  {}  {sha}{dirty}  {}{mark}",
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
    let store = PerfStore::new(ctx.store());
    let records = store.list()?;
    let baselines = store.baselines()?;
    let mut output = crate::host::stdout();
    if json {
        writeln!(output, "{}", serde_json::to_string_pretty(&records)?)?;
    } else if records.is_empty() {
        writeln!(output, "No performance records yet.")?;
    } else {
        writeln!(output, "id  subject  state  commit  machine")?;
        for record in &records {
            let marked = baselines.values().any(|id| *id == record.id());
            writeln!(output, "{}", row(record, marked))?;
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
    let mut output = crate::host::stdout();
    write!(output, "{}", render_timing(&record, options.top))?;
    writeln!(output, "\nSaved {}", path.display())?;
    Ok(())
}

/// Measure the named workflow on the project, save one record per state, and
/// print where each state's time went.
#[instrument(level = "debug", skip(ctx, program), err(level = "warn"))]
fn workflow(
    ctx: &ActCtx,
    name: &str,
    repeat: u32,
    top: usize,
    program: Option<PathBuf>,
) -> CordialResult<()> {
    let store_home = ctx
        .store_home()
        .clone()
        .unwrap_or_else(crate::default_store_home);
    let config = load_cordial_config(ctx.project_root(), &store_home);
    let edit = config
        .perf()
        .workflows()
        .get(name)
        .and_then(|workflow| workflow.edit().clone());
    let program = match program {
        Some(program) => program,
        None => std::env::current_exe()?,
    };
    let request = WorkflowRequestBuilder::default()
        .name(name)
        .program(program)
        .repeat(repeat)
        .edit(edit)
        .build()?;
    let store = PerfStore::new(ctx.store());
    let work = ctx.store().perf_dir().join("work");
    let mut output = crate::host::stdout();
    for record in measure_workflow(ctx.project_root(), &work, &request)? {
        let path = store.save(&record)?;
        write!(output, "{}", render_stages(&record, top))?;
        writeln!(output, "  Saved {}\n", path.display())?;
    }
    Ok(())
}

/// Run the set with per-test profiling, join the profiles into the record, save
/// it, and print where the time went.
#[instrument(level = "debug", skip(ctx, options), err(level = "warn"))]
fn profile(ctx: &ActCtx, name: &str, options: TimingOptions) -> CordialResult<()> {
    let store = PerfStore::new(ctx.store());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let directory = ctx
        .store()
        .perf_dir()
        .join("profiles")
        .join(format!("{stamp}-{name}"));
    std::fs::create_dir_all(&directory)?;
    let request = TimingRequest::new(
        name.to_owned(),
        NextestRequest::new(
            filter_of(ctx, name, options.filter)?,
            options.features,
            options.no_default_features,
            options.threads,
        )
        .with_env("CORDIAL_PERF_PROFILE_DIR", directory.display().to_string()),
        1,
    );
    let timed = measure(ctx.project_root(), &request, &store.raw_dir())?;
    let record = profiled(&timed, name, &read_profiles(&directory)?)?;
    let path = store.save(&record)?;
    let mut output = crate::host::stdout();
    write!(output, "{}", render_profile(&record, options.top))?;
    writeln!(output, "\nSaved {}", path.display())?;
    Ok(())
}

/// Mark the named record as its subject's baseline.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn baseline(ctx: &ActCtx, query: &str) -> CordialResult<()> {
    let store = PerfStore::new(ctx.store());
    let record = store.find(query)?;
    store.set_baseline(&record)?;
    let mut output = crate::host::stdout();
    writeln!(
        output,
        "Baseline for {}: {}",
        PerfStore::baseline_key(&record),
        record.id()
    )?;
    Ok(())
}

/// The (baseline, candidate) pairs the arguments name.
#[instrument(level = "debug", skip(store), err(level = "warn"))]
fn pairs(
    store: &PerfStore,
    first: Option<String>,
    second: Option<String>,
) -> CordialResult<Vec<(RunRecord, RunRecord)>> {
    let baselines = store.baselines()?;
    match (first, second) {
        (Some(before), Some(after)) => Ok(vec![(store.find(&before)?, store.find(&after)?)]),
        (Some(candidate), None) => {
            let candidate = store.find(&candidate)?;
            let key = PerfStore::baseline_key(&candidate);
            let id = baselines.get(&key).ok_or_else(|| {
                CordialError::perf(
                    store.path_of(&candidate.id()),
                    format!("no baseline for {key}; mark one with `cordial perf baseline`"),
                )
            })?;
            Ok(vec![(store.find(id)?, candidate)])
        }
        (None, _) => {
            let mut found = Vec::new();
            for id in baselines.values() {
                let baseline = store.find(id)?;
                let state = baseline.state();
                let subject = baseline.subject().to_string();
                if let Some(latest) = store.latest(&subject, state)?
                    && latest.id() != baseline.id()
                {
                    found.push((baseline, latest));
                }
            }
            Ok(found)
        }
    }
}

/// Compare the chosen records and print each comparison.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn compare_records(
    ctx: &ActCtx,
    first: Option<String>,
    second: Option<String>,
    (tolerance, top): (f64, usize),
    strict: bool,
) -> CordialResult<()> {
    let store = PerfStore::new(ctx.store());
    let pairs = pairs(&store, first, second)?;
    let mut output = crate::host::stdout();
    if pairs.is_empty() {
        writeln!(
            output,
            "Nothing to compare: no baseline has a newer record. Mark one with `cordial perf baseline <id>`."
        )?;
        return Ok(());
    }
    let mut regressed = Vec::new();
    for (before, after) in &pairs {
        let comparison = compare(before, after, tolerance);
        writeln!(output, "{}", render_comparison(&comparison, top))?;
        if comparison.verdict() == Verdict::Regressed {
            regressed.push(comparison.subject().clone());
        }
    }
    if strict && !regressed.is_empty() {
        return Err(CordialError::perf(
            ctx.store().perf_dir(),
            format!("regressed: {}", regressed.join(", ")),
        ));
    }
    Ok(())
}
