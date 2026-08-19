//! Clap subcommands. Each type implements `act` and hands off nested clap types.

use std::path::PathBuf;

use clap::Subcommand;

#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use super::run::execute_build_rustdoc;
#[cfg(feature = "homecoming_std")]
use super::run::execute_build_sysroot;
use super::run::{
    execute_run_plugins, execute_tracing_apply, export_surreal, list_exceptions, show_exceptions,
    view_store_file,
};
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use crate::RunAll;
#[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
use crate::coverage_plugins;
use crate::{CordialResult, StoreLayout, all_plugins, quality_plugins};
#[cfg(feature = "homecoming_std")]
use crate::{coverage_plugins_for_hub, discover_workspace_hub};
use tracing::instrument;

#[derive(Subcommand)]
pub enum Commands {
    /// Run all built-in etiquettes (quality + coverage).
    Run,
    /// Run source-quality etiquettes, or apply tracing instrument patches.
    Quality {
        /// Apply `#[instrument]` from the tracing checklist instead of running scanners.
        #[arg(long)]
        apply: bool,
        /// Log tracing apply changes without writing source files.
        #[arg(long)]
        dry_run: bool,
        /// Checklist path for `--apply` (default: `{store}/findings/tracing-instrument.checklist.md`).
        #[arg(long)]
        checklist: Option<PathBuf>,
    },
    /// Run rustdoc coverage etiquettes (impl coverage, trenchcoat, shadow).
    #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
    Coverage,
    /// Print a file from the project store to stdout.
    View {
        /// Path relative to the project store root (for example `findings/rollup-summary.md`).
        path: PathBuf,
    },
    /// Inspect or manage JSON patch exception files.
    Exceptions {
        #[command(subcommand)]
        command: ExceptionCommands,
    },
    /// Export cached IR for agent integration.
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
    /// Build rustdoc JSON and cache artifacts for coverage analysis.
    #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
    Build {
        #[command(subcommand)]
        command: BuildCommands,
    },
}

#[derive(Subcommand)]
pub enum ExceptionCommands {
    /// List exception patch files under the project store.
    List,
    /// Print one exception patch file as JSON.
    Show {
        /// Etiquette id (for example `panics`).
        etiquette: String,
        /// Crate name (default: derived from project directory).
        #[arg(long)]
        crate_name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ExportCommands {
    /// Export cached IR as SurrealDB-oriented JSON.
    Surreal {
        /// Write export to this path instead of stdout.
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        /// Emit SurrealQL CREATE/RELATE statements instead of JSON.
        #[arg(long)]
        statements: bool,
    },
}

#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
#[derive(Subcommand)]
pub enum BuildCommands {
    /// Build rustdoc JSON for workspace members and cache under the project store.
    #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
    Rustdoc {
        /// Rebuild even when a valid cached build artifact exists.
        #[arg(long)]
        force: bool,
    },
    /// Build rustdoc JSON for std-family sysroot libraries (`std`, `core`, `alloc`).
    #[cfg(feature = "homecoming_std")]
    Sysroot {
        /// Rebuild even when a valid cached build artifact exists.
        #[arg(long)]
        force: bool,
    },
}

pub(super) struct ActCtx {
    pub(super) project_root: PathBuf,
    pub(super) store: StoreLayout,
    pub(super) crate_name: Option<String>,
    pub(super) store_home: Option<PathBuf>,
}

impl Commands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: ActCtx) -> CordialResult<()> {
        match self {
            Self::Run => execute_run_plugins(
                &ctx.project_root,
                &ctx.store,
                ctx.crate_name.as_deref(),
                ctx.store_home.clone(),
                all_plugins(),
            ),
            Self::Quality {
                apply,
                dry_run,
                checklist,
            } => {
                if apply {
                    execute_tracing_apply(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        checklist.as_deref(),
                        dry_run,
                    )
                } else {
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        quality_plugins(),
                    )
                }
            }
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            Self::Coverage => {
                #[cfg(feature = "homecoming_std")]
                {
                    let hub = discover_workspace_hub(&ctx.project_root, &RunAll)?;
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        coverage_plugins_for_hub(hub),
                    )
                }
                #[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
                {
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        coverage_plugins(),
                    )
                }
            }
            Self::View { path } => view_store_file(&ctx.store, &path),
            Self::Exceptions { command } => command.act(&ctx),
            Self::Export { command } => command.act(&ctx),
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            Self::Build { command } => command.act(&ctx),
        }
    }
}

impl ExceptionCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::List => list_exceptions(&ctx.store),
            Self::Show {
                etiquette,
                crate_name,
            } => show_exceptions(
                &ctx.store,
                &etiquette,
                crate_name.as_deref().unwrap_or(&ctx.store.project_slug),
            ),
        }
    }
}

impl ExportCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::Surreal { output, statements } => export_surreal(
                &ctx.store,
                &ctx.project_root,
                ctx.crate_name.as_deref(),
                output.as_deref(),
                statements,
            ),
        }
    }
}

#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
impl BuildCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            Self::Rustdoc { force } => execute_build_rustdoc(
                &ctx.project_root,
                &ctx.store,
                ctx.crate_name.as_deref(),
                force,
            ),
            #[cfg(feature = "homecoming_std")]
            Self::Sysroot { force } => {
                execute_build_sysroot(ctx.store_home.clone(), ctx.crate_name.as_deref(), force)
            }
        }
    }
}
