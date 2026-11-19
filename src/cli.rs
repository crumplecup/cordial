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
        if self.memory_isolation && !crate::memory_limit::resumable() {
            // The run is a child of this process, and this one ends with what the last ended with.
            let program = std::env::current_exe()?;
            let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
            let code = crate::drive_resumable(&program, &arguments)?;
            std::process::exit(code);
        }
        let project_root = match &self.project {
            Some(path) => path.clone(),
            None => std::env::current_dir()?,
        };
        let slug = project_slug_from_path(&project_root);
        let store_home = self.store_home.clone().unwrap_or_else(default_store_home);
        let configured = crate::load_cordial_config(&project_root, &store_home);
        let limit =
            crate::resolve_memory_limit(self.memory_limit, configured.memory_limit().as_deref());
        crate::enforce(crate::resolve_memory_target(
            limit,
            self.memory_target,
            configured.memory_target().as_deref(),
            crate::available_memory(),
        ));
        let store_root = store_home.join(&slug);
        let store = StoreLayout::from_root(store_root.clone(), slug);
        // The guard spans command execution and writes its report on drop.
        #[cfg(feature = "hotpath")]
        let _hotpath = {
            let config = crate::load_cordial_config(&project_root, &store_home);
            let path = config
                .hotpath()
                .output_path()
                .as_ref()
                .map(|configured| {
                    if configured.is_absolute() {
                        configured.clone()
                    } else {
                        store_root.join(configured)
                    }
                })
                .unwrap_or_else(|| {
                    std::env::temp_dir()
                        .join(format!("cordial-hotpath-{}.json", std::process::id()))
                });
            tracing::info!(path = %path.display(), "hotpath report");
            // Held where the memory watchdog can write the report too: it exits the process, which
            // does not drop what a function holds.
            crate::profiling::hold(crate::profiling::guard("cordial", path));
            crate::profiling::Held
        };
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
        self.command.act(ActCtx::new(
            project_root,
            store,
            self.crate_name,
            self.store_home,
            progress,
            self.stop_after,
        ))
    }
}
