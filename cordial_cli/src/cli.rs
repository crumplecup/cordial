use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::{CliError, CliResult};
use clap::{Parser, Subcommand};
#[cfg(feature = "elicitation")]
use cordial::build_all_active_shadow_deps;
#[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
use cordial::coverage_plugins;
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use cordial::{BuildOptions, build_workspace_members};
use cordial::{
    CrateIr, Disposition, NamedRunFilter, Plugin, RunAll, RunFilter, RunOutcome, Session,
    SessionBuilder, StoreLayout, SurrealGraphExport, all_plugins, load_exceptions,
    project_slug_from_path, quality_plugins, run_tracing_instrument_apply,
};
#[cfg(feature = "homecoming_std")]
use cordial::{
    SysrootCache, build_sysroot_libraries, coverage_plugins_for_hub, discover_workspace_hub,
};

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

    #[command(subcommand)]
    pub command: Commands,
}

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

pub fn run() -> CliResult<()> {
    let cli = Cli::parse();
    let project_root = match &cli.project {
        Some(path) => path.clone(),
        None => std::env::current_dir()?,
    };
    let slug = project_slug_from_path(&project_root);
    let store_root = cli
        .store_home
        .clone()
        .unwrap_or_else(cordial::default_store_home)
        .join(&slug);
    let store = StoreLayout::from_root(store_root, slug);

    match cli.command {
        Commands::Run => execute_run_plugins(&project_root, &store, &cli, all_plugins()),
        Commands::Quality {
            apply,
            dry_run,
            ref checklist,
        } => {
            if apply {
                execute_tracing_apply(&project_root, &store, &cli, checklist.as_deref(), dry_run)
            } else {
                execute_run_plugins(&project_root, &store, &cli, quality_plugins())
            }
        }
        #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
        Commands::Coverage => {
            #[cfg(feature = "homecoming_std")]
            {
                let hub = discover_workspace_hub(&project_root, &RunAll)?;
                execute_run_plugins(&project_root, &store, &cli, coverage_plugins_for_hub(hub))
            }
            #[cfg(all(feature = "elicitation", not(feature = "homecoming_std")))]
            {
                execute_run_plugins(&project_root, &store, &cli, coverage_plugins())
            }
        }
        #[cfg(all(not(feature = "elicitation"), not(feature = "homecoming_std")))]
        Commands::Coverage => Err(CliError::CoverageFeatureDisabled),
        Commands::View { path } => view_store_file(&store, &path),
        Commands::Exceptions { command } => match command {
            ExceptionCommands::List => list_exceptions(&store),
            ExceptionCommands::Show {
                etiquette,
                crate_name,
            } => show_exceptions(
                &store,
                &etiquette,
                crate_name.as_deref().unwrap_or(&store.project_slug),
            ),
        },
        Commands::Export { command } => match command {
            ExportCommands::Surreal { output, statements } => export_surreal(
                &store,
                &project_root,
                cli.crate_name.as_deref(),
                output.as_deref(),
                statements,
            ),
        },
        #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
        Commands::Build { ref command } => match command {
            #[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
            BuildCommands::Rustdoc { force } => {
                execute_build_rustdoc(&project_root, &store, &cli, *force)
            }
            #[cfg(feature = "homecoming_std")]
            BuildCommands::Sysroot { force } => execute_build_sysroot(&cli, *force),
        },
    }
}

fn execute_build_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    cli: &Cli,
    force: bool,
) -> CliResult<()> {
    let artifacts = build_workspace_members(
        project_root,
        store,
        cli.crate_name.as_deref(),
        &BuildOptions { force },
    )?;
    for artifact in artifacts {
        eprintln!(
            "built {} -> {}",
            artifact.crate_name,
            artifact.rustdoc_json.display()
        );
    }
    #[cfg(feature = "elicitation")]
    if let Ok(shadow_dep_artifacts) =
        build_all_active_shadow_deps(project_root, store, &BuildOptions { force })
    {
        for artifact in shadow_dep_artifacts {
            eprintln!(
                "built shadow-dep {} (via {}) -> {}",
                artifact.crate_name,
                artifact.reference_member.as_deref().unwrap_or("unknown"),
                artifact.rustdoc_json.display()
            );
        }
    }
    Ok(())
}

#[cfg(feature = "homecoming_std")]
fn execute_build_sysroot(cli: &Cli, force: bool) -> CliResult<()> {
    let home = cli
        .store_home
        .clone()
        .unwrap_or_else(cordial::default_store_home);
    let sysroot = SysrootCache::from_home(home);
    let artifacts =
        build_sysroot_libraries(&sysroot, cli.crate_name.as_deref(), &BuildOptions { force })?;
    for artifact in artifacts {
        eprintln!(
            "built {} -> {}",
            artifact.crate_name,
            sysroot.rustdoc_cache_path(&artifact.crate_name).display()
        );
    }
    Ok(())
}

fn execute_tracing_apply(
    project_root: &Path,
    store: &StoreLayout,
    cli: &Cli,
    checklist: Option<&Path>,
    dry_run: bool,
) -> CliResult<()> {
    let checklist_path = checklist
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.findings_dir().join("tracing-instrument.checklist.md"));
    let summary = run_tracing_instrument_apply(
        project_root,
        &checklist_path,
        cli.crate_name.as_deref(),
        dry_run,
    )?;
    eprintln!(
        "tracing apply: {} functions in {} files ({} skipped, {} unresolved)",
        summary.changed_functions,
        summary.changed_files,
        summary.skipped_existing,
        summary.unresolved,
    );
    Ok(())
}

fn execute_run_plugins(
    project_root: &Path,
    store: &StoreLayout,
    cli: &Cli,
    plugins: Vec<&'static dyn Plugin>,
) -> CliResult<()> {
    let mut builder = SessionBuilder::new(project_root)
        .with_store_root(store.root.clone())
        .with_store_home(
            cli.store_home
                .clone()
                .unwrap_or_else(cordial::default_store_home),
        );
    for plugin in plugins {
        builder = builder.register_plugin(plugin);
    }

    let session = builder.build();
    let filter = run_filter(cli);
    let outcome = session.run(filter.as_ref())?;
    print_run_summary(outcome.as_ref());
    Ok(())
}

fn run_filter(cli: &Cli) -> RunFilterChoice {
    match &cli.crate_name {
        Some(crate_name) => {
            RunFilterChoice::Named(NamedRunFilter::all_plugins().with_crate(crate_name.clone()))
        }
        None => RunFilterChoice::All(RunAll),
    }
}

enum RunFilterChoice {
    All(RunAll),
    Named(NamedRunFilter),
}

impl RunFilterChoice {
    fn as_ref(&self) -> &dyn RunFilter {
        match self {
            Self::All(filter) => filter,
            Self::Named(filter) => filter,
        }
    }
}

fn print_run_summary(outcome: &dyn RunOutcome) {
    let mut open = 0usize;
    let mut suppressed = 0usize;
    for finding in outcome.findings() {
        match finding.disposition() {
            Disposition::Open | Disposition::Exemplar => open += 1,
            Disposition::Suppressed => suppressed += 1,
        }
    }
    let artifacts: Vec<_> = outcome
        .artifacts()
        .map(|artifact| artifact.name())
        .collect();
    eprintln!("findings: {open} open, {suppressed} suppressed");
    if !artifacts.is_empty() {
        eprintln!("artifacts:");
        for name in artifacts {
            eprintln!("  {name}");
        }
    }
}

fn view_store_file(store: &StoreLayout, path: &Path) -> CliResult<()> {
    let full = store.root.join(path);
    if !full.is_file() {
        return Err(CliError::NotFound { path: full });
    }
    let bytes = fs::read(&full)?;
    io::stdout().write_all(&bytes)?;
    Ok(())
}

fn list_exceptions(store: &StoreLayout) -> CliResult<()> {
    let mut files = Vec::new();
    let exceptions_dir = store.exceptions_dir();
    if exceptions_dir.is_dir() {
        collect_files(&exceptions_dir, &exceptions_dir, &mut files, "exceptions")?;
    }
    let patches_dir = store.quality_patches_dir();
    if patches_dir.is_dir() {
        collect_files(&patches_dir, &patches_dir, &mut files, "quality/patches")?;
    }
    files.sort();
    if files.is_empty() {
        eprintln!(
            "no exception files under {} or {}",
            exceptions_dir.display(),
            patches_dir.display()
        );
        return Ok(());
    }
    for path in files {
        println!("{}", path.display());
    }
    Ok(())
}

fn collect_files(
    base: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
    prefix: &str,
) -> CliResult<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out, prefix)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let rel = path.strip_prefix(base)?;
            out.push(PathBuf::from(prefix).join(rel));
        }
    }
    Ok(())
}

fn show_exceptions(store: &StoreLayout, etiquette: &str, crate_name: &str) -> CliResult<()> {
    let set = load_exceptions(store, etiquette, crate_name)?;
    if set.is_empty() {
        return Err(CliError::NoExceptions {
            etiquette: etiquette.to_string(),
            crate_name: crate_name.to_string(),
        });
    }
    let file_name = format!("{crate_name}.json");
    let canonical = store.exceptions_dir().join(etiquette).join(&file_name);
    let alias = store.quality_patches_dir().join(etiquette).join(&file_name);
    let path = if canonical.is_file() {
        canonical
    } else {
        alias
    };
    let bytes = fs::read(&path)?;
    io::stdout().write_all(&bytes)?;
    if !bytes.ends_with(b"\n") {
        println!();
    }
    Ok(())
}

fn export_surreal(
    store: &StoreLayout,
    project_root: &Path,
    crate_name: Option<&str>,
    output: Option<&Path>,
    statements: bool,
) -> CliResult<()> {
    let crate_name = crate_name
        .map(str::to_string)
        .or_else(|| {
            cordial::discover_crate_targets(project_root, &NamedRunFilter::all_plugins())
                .ok()
                .and_then(|targets| targets.into_iter().next().map(|t| t.crate_name))
        })
        .unwrap_or_else(|| store.project_slug.clone());
    let cache_path = store.ir_cache_path(&crate_name);
    if !cache_path.is_file() {
        return Err(CliError::NoCachedIr { path: cache_path });
    }
    let ir = CrateIr::read_cache(&cache_path)?;
    let export = SurrealGraphExport::from_crate_ir(&ir)?;
    let body = if statements {
        cordial::surreal_statements(&export).join("\n") + "\n"
    } else {
        export.to_json_pretty()?
    };

    if let Some(path) = output {
        fs::write(path, body)?;
        eprintln!("wrote {}", path.display());
    } else {
        print!("{body}");
    }
    Ok(())
}
