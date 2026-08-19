//! CLI command bodies. Clap types in `cli` call these after `act` dispatch.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(feature = "elicitation")]
use crate::build_all_active_shadow_deps;
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use crate::{BuildOptions, build_workspace_members};
use crate::{
    CordialError, CordialResult, CrateIr, Disposition, NamedRunFilter, Plugin, RunAll, RunFilter,
    RunOutcome, Session, SessionBuilder, StoreLayout, SurrealGraphExport, default_store_home,
    load_exceptions, run_tracing_instrument_apply,
};
#[cfg(feature = "homecoming_std")]
use crate::{SysrootCache, build_sysroot_libraries};
use tracing::instrument;

#[instrument(level = "debug", skip(store), err(level = "warn"))]
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
pub(super) fn execute_build_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    force: bool,
) -> CordialResult<()> {
    let artifacts =
        build_workspace_members(project_root, store, crate_name, &BuildOptions { force })?;
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

#[instrument(level = "debug", err(level = "warn"))]
#[cfg(feature = "homecoming_std")]
pub(super) fn execute_build_sysroot(
    store_home: Option<PathBuf>,
    crate_name: Option<&str>,
    force: bool,
) -> CordialResult<()> {
    let home = store_home.unwrap_or_else(default_store_home);
    let sysroot = SysrootCache::from_home(home);
    let artifacts = build_sysroot_libraries(&sysroot, crate_name, &BuildOptions { force })?;
    for artifact in artifacts {
        eprintln!(
            "built {} -> {}",
            artifact.crate_name,
            sysroot.rustdoc_cache_path(&artifact.crate_name).display()
        );
    }
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_tracing_apply(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    checklist: Option<&Path>,
    dry_run: bool,
) -> CordialResult<()> {
    let checklist_path = checklist
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.findings_dir().join("tracing-instrument.checklist.md"));
    let summary = run_tracing_instrument_apply(project_root, &checklist_path, crate_name, dry_run)?;
    eprintln!(
        "tracing apply: {} functions in {} files ({} skipped, {} unresolved)",
        summary.changed_functions,
        summary.changed_files,
        summary.skipped_existing,
        summary.unresolved,
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, plugins), err(level = "warn"))]
pub(super) fn execute_run_plugins(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    store_home: Option<PathBuf>,
    plugins: Vec<&'static dyn Plugin>,
) -> CordialResult<()> {
    let mut builder = SessionBuilder::new(project_root)
        .with_store_root(store.root.clone())
        .with_store_home(store_home.unwrap_or_else(default_store_home));
    for plugin in plugins {
        builder = builder.register_plugin(plugin);
    }

    let session = builder.build();
    let filter = run_filter(crate_name);
    let outcome = session.run(filter.as_ref())?;
    print_run_summary(outcome.as_ref());
    Ok(())
}

#[instrument(level = "info", fields(crate_name = crate_name))]
fn run_filter(crate_name: Option<&str>) -> RunFilterChoice {
    match crate_name {
        Some(crate_name) => {
            RunFilterChoice::Named(NamedRunFilter::all_plugins().with_crate(crate_name.to_string()))
        }
        None => RunFilterChoice::All(RunAll),
    }
}

enum RunFilterChoice {
    All(RunAll),
    Named(NamedRunFilter),
}

impl RunFilterChoice {
    #[instrument(level = "trace", skip(self))]
    fn as_ref(&self) -> &dyn RunFilter {
        match self {
            Self::All(filter) => filter,
            Self::Named(filter) => filter,
        }
    }
}

#[instrument(level = "debug", skip(outcome))]
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

#[instrument(level = "debug", skip(store, path), err(level = "warn"))]
pub(super) fn view_store_file(store: &StoreLayout, path: &Path) -> CordialResult<()> {
    let full = store.root.join(path);
    if !full.is_file() {
        return Err(CordialError::not_found(full));
    }
    let bytes = fs::read(&full)?;
    io::stdout().write_all(&bytes)?;
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn list_exceptions(store: &StoreLayout) -> CordialResult<()> {
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

#[instrument(level = "debug", err(level = "warn"))]
fn collect_files(
    base: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
    prefix: &str,
) -> CordialResult<()> {
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

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn show_exceptions(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
) -> CordialResult<()> {
    let set = load_exceptions(store, etiquette, crate_name)?;
    if set.is_empty() {
        return Err(CordialError::no_exceptions(etiquette, crate_name));
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

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn export_surreal(
    store: &StoreLayout,
    project_root: &Path,
    crate_name: Option<&str>,
    output: Option<&Path>,
    statements: bool,
) -> CordialResult<()> {
    let crate_name = crate_name
        .map(str::to_string)
        .or_else(|| {
            crate::discover_crate_targets(project_root, &NamedRunFilter::all_plugins())
                .ok()
                .and_then(|targets| targets.into_iter().next().map(|t| t.crate_name))
        })
        .unwrap_or_else(|| store.project_slug.clone());
    let cache_path = store.ir_cache_path(&crate_name);
    if !cache_path.is_file() {
        return Err(CordialError::no_cached_ir(cache_path));
    }
    let ir = CrateIr::read_cache(&cache_path)?;
    let export = SurrealGraphExport::from_crate_ir(&ir)?;
    let body = if statements {
        crate::surreal_statements(&export).join("\n") + "\n"
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
