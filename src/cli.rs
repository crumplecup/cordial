//! Clap types and dispatch. `main` parses and calls [`Cli::act`].

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::instrument;

use crate::{
    CordialResult, LineProgress, ProgressSink, StoreLayout, default_store_home,
    project_slug_from_path,
};

mod base;
mod clean;
mod commands;
mod exception_args;
mod exceptions;
mod export;
mod gate_args;
mod perf;
mod progress;
mod run;
mod tiers;
mod timings;
#[cfg(feature = "quality")]
mod worklist;

use commands::ActCtx;
pub use commands::Commands;
use progress::IndicatifProgress;

/// Top-level clap parser for the `cordial` binary.
#[derive(Parser)]
#[command(
    name = "cordial",
    about = "Polite standards for code development",
    version
)]
pub struct Cli {
    /// Project root to analyze (default: current directory).
    #[arg(long, short = 'p', env = "CORDIAL_PROJECT", global = true)]
    pub project: Option<PathBuf>,

    /// Store home directory (default: `~/.cordial`).
    #[arg(long, env = "CORDIAL_HOME", global = true)]
    pub store_home: Option<PathBuf>,

    /// Restrict analysis to one crate name (default: project directory name).
    #[arg(long, global = true)]
    pub crate_name: Option<String>,

    /// End `open` or `submit` cleanly after one pipeline stage and log the
    /// timings so far, to measure a stage without running the rest.
    #[arg(long, global = true, value_parser = clap::builder::PossibleValuesParser::new(crate::STAGES))]
    pub stop_after: Option<String>,

    /// Stop the run, with a message saying where it was, when this process uses more memory
    /// than this: a size such as 12G or 512M, a share of the memory available when it starts
    /// such as 75%, or 0 for no limit (default: 75%, or `memory_limit` in `cordial.toml`).
    #[arg(
        long,
        env = "CORDIAL_MEMORY_LIMIT",
        global = true,
        value_parser = crate::parse_limit
    )]
    pub memory_limit: Option<crate::MemoryLimit>,

    /// The memory the pressure ladder (evicting, spilling, dropping what can be computed again)
    /// works to, when that is less than the limit: the run is stopped at `--memory-limit` and not
    /// here, so that how it behaves under pressure can be seen all the way to the limit. A size
    /// such as 1536M or a share such as 20% (default: the limit, or `memory_target` in
    /// `cordial.toml`).
    #[arg(
        long,
        env = "CORDIAL_MEMORY_TARGET",
        global = true,
        value_parser = crate::parse_limit
    )]
    pub memory_target: Option<crate::MemoryLimit>,

    /// How early the run lets go of the scaffolding it builds (trees, expansions, span maps), as
    /// the share of the budget a process may hold before it does: lean (15%), balanced (30%),
    /// roomy (50%), off (only under pressure), or 0 to 100 (default: balanced, or
    /// `memory_parsimony` in `cordial.toml`).
    #[arg(
        long,
        env = "CORDIAL_MEMORY_PARSIMONY",
        global = true,
        value_parser = crate::parse_parsimony
    )]
    pub memory_parsimony: Option<u64>,

    /// Run in processes that are taken up again: the run saves its state and exits when its
    /// resident memory nears the budget, and a new process restores it and goes on, so that the
    /// memory an allocator kept goes back to the machine (default: off).
    #[arg(long, env = "CORDIAL_MEMORY_ISOLATION", global = true)]
    pub memory_isolation: bool,

    /// Nested clap subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Build session context and dispatch the selected [`Commands`] variant.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn act(self) -> CordialResult<()> {
        let code = self.act_with_exit_code()?;
        if code != 0 {
            crate::profiling::finish(crate::profiling::FinishReason::Error);
            std::process::exit(code);
        }
        Ok(())
    }

    /// Dispatch and return the isolation driver's exit code after its stack has unwound.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn act_with_exit_code(self) -> CordialResult<i32> {
        let project_root = match &self.project {
            Some(path) => path.clone(),
            None => std::env::current_dir()?,
        };
        let slug = project_slug_from_path(&project_root);
        let store_home = self.store_home.clone().unwrap_or_else(default_store_home);
        let store_root = store_home.join(&slug);
        let store = StoreLayout::from_root(store_root.clone(), slug);
        let configured = crate::load_cordial_config(&project_root, &store_home);
        let driver = self.memory_isolation && !crate::memory_limit::resumable();
        let export = configured.hotpath().output_path().as_ref().map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                store_root.join(path)
            }
        });
        crate::profiling::configure(
            &store.perf_dir().join("raw/profiles"),
            export.as_deref(),
            crate::profiling::Identity {
                role: if driver {
                    "isolation_driver"
                } else if crate::memory_limit::resumable() {
                    "isolation_child"
                } else {
                    "cli"
                }
                .to_owned(),
                boundary: "process_entry".to_owned(),
                ..crate::profiling::Identity::default()
            },
        )?;
        if driver {
            let program = std::env::current_exe()?;
            let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
            let results = store_root.join("run-results");
            let code = crate::drive_resumable(&program, &arguments, &results)?;
            return Ok(code);
        }
        // What a process of a run that keeps its results in a store has gathered is written when it
        // ends (the store itself is opened once the workspace is loaded, and named for its inputs).
        let _results = crate::result_store::FlushOnDrop;
        let limit =
            crate::resolve_memory_limit(self.memory_limit, configured.memory_limit().as_deref());
        // The budget is the whole run's: a process of a run in processes spends what is left of it
        // when the driver, which is part of the run, has what it holds.
        let budget = crate::resolve_memory_target(
            limit,
            self.memory_target,
            configured.memory_target().as_deref(),
            crate::available_memory(),
        )
        .reserving(crate::reserved_by_driver());
        crate::enforce(budget.clone());
        crate::be_parsimonious_at(crate::resolve_parsimony(
            self.memory_parsimony,
            configured.memory_parsimony().as_deref(),
        ));
        let _footprint = crate::FootprintOnDrop;
        #[cfg(feature = "quality")]
        let json_worklist = matches!(
            &self.command,
            Commands::Open { json: true }
                | Commands::Peek { json: true }
                | Commands::Done { json: true, .. }
                | Commands::Skip { json: true, .. }
                | Commands::Submit { json: true, .. }
                | Commands::Stats { json: true, .. }
        );
        #[cfg(not(feature = "quality"))]
        let json_worklist = false;
        let progress: Arc<dyn ProgressSink> = if std::env::var_os("CORDIAL_NO_PROGRESS").is_some() {
            Arc::new(IndicatifProgress::hidden())
        } else if std::io::stderr().is_terminal() && !json_worklist {
            Arc::new(IndicatifProgress::new())
        } else {
            Arc::new(LineProgress::new())
        };
        self.command
            .act(ActCtx::new(
                project_root,
                store,
                self.crate_name,
                self.store_home,
                progress,
                self.stop_after,
                budget,
            ))
            .map(|()| 0)
    }
}
