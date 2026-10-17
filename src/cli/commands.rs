//! Clap subcommands. Each type implements `act` and hands off nested clap types.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Subcommand;

use super::exceptions::{
    execute_add_coverage_skip, execute_add_exception, execute_backup_exceptions,
    execute_edit_exception, execute_load_exceptions, execute_remove_exception, list_exceptions,
    show_exceptions,
};
use super::run::{execute_quality_apply, execute_run_plugins, view_store_file};
use crate::{
    CordialError, CordialResult, CoverageSkipEntry, DEFAULT_EXCEPTIONS_REGISTRY, ExceptionEntry,
    ProgressSink, StoreLayout, all_plugins, quality_plugins,
};
use tracing::instrument;

/// Top-level `cordial` subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Show the current worklist item, building a list when needed.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::OPEN_HELP)]
    Open,
    /// Show the current item without building a list.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::PEEK_HELP)]
    Peek,
    /// Check off named items, or the current item.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::DONE_HELP)]
    Done {
        /// Stable item ids; omit to check off the current item.
        ids: Vec<String>,
    },
    /// Set named items aside, or the current item.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::SKIP_HELP)]
    Skip {
        /// Stable item ids; omit to set aside the current item.
        ids: Vec<String>,
    },
    /// Recheck the project and score claims against a fresh worklist.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::SUBMIT_HELP)]
    Submit {
        /// Fail if the new list still has open items.
        #[arg(long)]
        deny_open: bool,
    },
    /// Suggest the commands that move through a worklist.
    #[cfg(feature = "quality")]
    #[command(hide = true)]
    Next,
    /// Show worklist counts or individual items.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::STATS_HELP)]
    Stats {
        /// Show items and their checked state.
        #[arg(long)]
        verbose: bool,
        /// Restrict the verbose view to one area title.
        #[arg(long, requires = "verbose")]
        area: Option<String>,
        /// Show every item instead of the first 25.
        #[arg(long, requires = "verbose")]
        all: bool,
    },
    /// Print the agent worklist loop and verification rules.
    #[cfg(feature = "quality")]
    #[command(about = crate::worklist::INSTRUCTIONS_HELP)]
    Instructions,
    /// Run all built-in etiquettes (quality + coverage).
    Run {
        /// Exit with failure when unresolved open findings remain.
        #[arg(long)]
        deny_open: bool,
    },
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
        /// Exit with failure when unresolved open findings remain.
        #[arg(long, conflicts_with = "apply")]
        deny_open: bool,
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
    /// Print why an etiquette exists and how to opt out.
    Explain {
        /// Etiquette id or rule id (`doc_warnings`, `DOC-WARNING-001`).
        /// Omit to list every etiquette compiled into this binary.
        id: Option<String>,
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
    /// Delete exactly one quality exception row (errors on zero or several matches).
    Remove {
        /// Which row to remove.
        #[command(flatten)]
        target: ExceptionTargetArgs,
    },
    /// Correct one quality exception row in place (errors on zero or several matches).
    Edit {
        /// Which row to edit.
        #[command(flatten)]
        target: ExceptionTargetArgs,
        /// New values for the selected row.
        #[command(flatten)]
        new: ExceptionNewArgs,
    },
}

pub use super::exception_args::{ExceptionNewArgs, ExceptionTargetArgs};

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

#[derive(derive_new::new, derive_getters::Getters)]
pub(super) struct ActCtx {
    project_root: PathBuf,
    store: StoreLayout,
    crate_name: Option<String>,
    store_home: Option<PathBuf>,
    progress: Arc<dyn ProgressSink>,
}

impl Commands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: ActCtx) -> CordialResult<()> {
        match self {
            #[cfg(feature = "quality")]
            command @ (Self::Open
            | Self::Peek
            | Self::Done { .. }
            | Self::Skip { .. }
            | Self::Submit { .. }
            | Self::Next
            | Self::Stats { .. }
            | Self::Instructions) => command.act_worklist(&ctx),
            Self::Run { deny_open } => execute_run_plugins(
                &ctx.project_root,
                &ctx.store,
                ctx.crate_name.as_deref(),
                ctx.store_home.clone(),
                ctx.progress.clone(),
                all_plugins(),
                deny_open,
                true,
                "run",
            ),
            Self::Quality {
                apply,
                dry_run,
                checklist,
                deny_open,
            } => {
                if apply {
                    execute_quality_apply(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        checklist.as_deref(),
                        ctx.progress.clone(),
                        dry_run,
                    )
                } else {
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        ctx.progress.clone(),
                        quality_plugins(),
                        deny_open,
                        true,
                        "quality",
                    )
                }
            }
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            Self::Coverage => {
                #[cfg(feature = "homecoming_std")]
                {
                    let hub = crate::discover_workspace_hub(&ctx.project_root, &crate::RunAll)?;
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        ctx.progress.clone(),
                        crate::coverage_plugins_for_hub(hub),
                        false,
                        false,
                        "coverage",
                    )
                }
                #[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
                {
                    execute_run_plugins(
                        &ctx.project_root,
                        &ctx.store,
                        ctx.crate_name.as_deref(),
                        ctx.store_home.clone(),
                        ctx.progress.clone(),
                        crate::coverage_plugins(),
                        false,
                        false,
                        "coverage",
                    )
                }
            }
            Self::View { path } => view_store_file(&ctx.store, &path),
            Self::Exceptions { command } => command.act(&ctx),
            Self::Export { command } => command.act(&ctx),
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            Self::Build { command } => command.act(&ctx),
            Self::Explain { id } => super::run::execute_explain(
                &ctx.project_root,
                ctx.store_home.clone(),
                id.as_deref(),
            ),
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
                crate_name.as_deref().unwrap_or(ctx.store.project_slug()),
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
                        .unwrap_or_else(|| ctx.store.project_slug().clone());
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
            Self::Remove { target } => execute_remove_exception(
                &ctx.store,
                &target.etiquette,
                &target.crate_name(ctx),
                &target.selector(),
            ),
            Self::Edit { target, new } => execute_edit_exception(
                &ctx.store,
                &target.etiquette,
                &target.crate_name(ctx),
                &target.selector(),
                &new.update(),
            ),
        }
    }
}

#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
impl BuildCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::Rustdoc { force } => super::run::execute_build_rustdoc(
                &ctx.project_root,
                &ctx.store,
                ctx.crate_name.as_deref(),
                ctx.progress.clone(),
                force,
            ),
            #[cfg(feature = "homecoming_std")]
            Self::Sysroot { force } => super::run::execute_build_sysroot(
                ctx.store_home.clone(),
                ctx.crate_name.as_deref(),
                ctx.progress.clone(),
                force,
            ),
        }
    }
}
