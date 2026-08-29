//! Clap subcommands. Each type implements `act` and hands off nested clap types.

use std::path::PathBuf;

use clap::Subcommand;

#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use super::run::execute_build_rustdoc;
#[cfg(feature = "homecoming_std")]
use super::run::execute_build_sysroot;
use super::run::{
    execute_add_coverage_skip, execute_add_exception, execute_backup_exceptions,
    execute_load_exceptions, execute_quality_apply, execute_run_plugins, export_surreal,
    list_exceptions, show_exceptions, view_store_file,
};
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use crate::RunAll;
#[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
use crate::coverage_plugins;
use crate::{
    CordialError, CordialResult, CoverageSkipEntry, DEFAULT_EXCEPTIONS_REGISTRY, ExceptionEntry,
    StoreLayout, all_plugins, quality_plugins,
};
#[cfg(feature = "homecoming_std")]
use crate::{coverage_plugins_for_hub, discover_workspace_hub};
use tracing::instrument;

/// Top-level `cordial` subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Run all built-in etiquettes (quality + coverage).
    Run,
    /// Run source-quality etiquettes, or apply mechanical patches.
    Quality {
        /// Write tracing `#[instrument]` recipes and crate-root lint attributes.
        #[arg(long)]
        apply: bool,
        /// Log apply changes without writing source files.
        #[arg(long)]
        dry_run: bool,
        /// Tracing instrument checklist (default:
        /// `{store}/findings/tracing-instrument.checklist.md`). Crate-attrs
        /// apply scans library roots and does not use this path.
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
        /// Nested clap subcommand.
        #[command(subcommand)]
        command: ExceptionCommands,
    },
    /// Export cached IR for agent integration.
    Export {
        /// Nested clap subcommand.
        #[command(subcommand)]
        command: ExportCommands,
    },
    /// Build rustdoc JSON and cache artifacts for coverage analysis.
    #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
    Build {
        /// Nested clap subcommand.
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
    /// Backup curated exception files into `{root}/{slug}/...`.
    Backup {
        /// Registry root that receives the slug-scoped backup tree.
        #[arg(default_value = DEFAULT_EXCEPTIONS_REGISTRY)]
        root: PathBuf,
    },
    /// Load curated exception files from `{root}/{slug}/...` into the store.
    Load {
        /// Registry root containing the slug-scoped backup tree.
        #[arg(default_value = DEFAULT_EXCEPTIONS_REGISTRY)]
        root: PathBuf,
    },
    /// Append one exception row to the project store (quality or coverage skip).
    Add {
        /// Quality etiquette id (for example `panics`). Omit when using `--patch-set`.
        #[arg(required_unless_present = "patch_set")]
        etiquette: Option<String>,
        /// Source file relative to the crate root.
        #[arg(long, required_unless_present = "patch_set")]
        file: Option<String>,
        /// Only match findings on this line.
        #[arg(long)]
        line: Option<u32>,
        /// Only match findings with this rule id.
        #[arg(long)]
        rule_id: Option<String>,
        /// Only match findings with this context / qualified name.
        #[arg(long)]
        context: Option<String>,
        /// Crate name (default: `--crate-name` or the project directory).
        #[arg(long)]
        crate_name: Option<String>,
        /// Coverage skip list name (`chrono`, `{crate}-shadow`).
        #[arg(
            long,
            conflicts_with_all = ["etiquette", "file", "line", "rule_id", "context", "crate_name"]
        )]
        patch_set: Option<String>,
        /// Qualified path for a coverage skip.
        #[arg(long, requires = "patch_set", required_unless_present = "etiquette")]
        path: Option<String>,
        /// Human-readable explanation shown in reports.
        #[arg(long)]
        reason: String,
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

#[derive(derive_new::new)]
pub(super) struct ActCtx {
    project_root: PathBuf,
    store: StoreLayout,
    crate_name: Option<String>,
    store_home: Option<PathBuf>,
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
                    execute_quality_apply(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
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
            Self::Backup { root } => {
                execute_backup_exceptions(&ctx.project_root, &ctx.store, &root)
            }
            Self::Load { root } => execute_load_exceptions(&ctx.project_root, &ctx.store, &root),
            Self::Add {
                etiquette,
                file,
                line,
                rule_id,
                context,
                crate_name,
                patch_set,
                path,
                reason,
            } => {
                if let Some(patch_set) = patch_set {
                    let path = path
                        .ok_or_else(|| CordialError::invariant("coverage skip requires --path"))?;
                    execute_add_coverage_skip(
                        &ctx.store,
                        &patch_set,
                        CoverageSkipEntry::new(path, reason),
                    )
                } else {
                    let etiquette = etiquette.ok_or_else(|| {
                        CordialError::invariant("quality exception requires an etiquette")
                    })?;
                    let file = file.ok_or_else(|| {
                        CordialError::invariant("quality exception requires --file")
                    })?;
                    let crate_name = crate_name
                        .or_else(|| ctx.crate_name.clone())
                        .unwrap_or_else(|| ctx.store.project_slug.clone());
                    let mut entry = ExceptionEntry::new(file, reason);
                    if let Some(line) = line {
                        entry = entry.with_line(line);
                    }
                    if let Some(rule_id) = rule_id {
                        entry = entry.with_rule_id(rule_id);
                    }
                    if let Some(context) = context {
                        entry = entry.with_context(context);
                    }
                    execute_add_exception(&ctx.store, &etiquette, &crate_name, entry)
                }
            }
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
