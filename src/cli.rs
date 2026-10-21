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

    /// Nested clap subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Build session context and dispatch the selected [`Commands`] variant.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub fn act(self) -> CordialResult<()> {
        let project_root = match &self.project {
            Some(path) => path.clone(),
            None => std::env::current_dir()?,
        };
        let slug = project_slug_from_path(&project_root);
        let store_home = self.store_home.clone().unwrap_or_else(default_store_home);
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
            hotpath::HotpathGuardBuilder::new("cordial")
                .format(hotpath::Format::JsonPretty)
                .output_path(path)
                .functions_limit(0)
                .build()
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
        ))
    }
}
