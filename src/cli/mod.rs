//! Clap types and dispatch. `main` parses and calls [`Cli::act`].

use std::path::PathBuf;

use clap::Parser;
use tracing::instrument;

use crate::{CordialResult, StoreLayout, default_store_home, project_slug_from_path};

mod commands;
mod run;

use commands::ActCtx;
pub use commands::Commands;

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
        let store_root = self
            .store_home
            .clone()
            .unwrap_or_else(default_store_home)
            .join(&slug);
        let store = StoreLayout::from_root(store_root, slug);
        self.command.act(ActCtx::new(
            project_root,
            store,
            self.crate_name,
            self.store_home,
        ))
    }
}
