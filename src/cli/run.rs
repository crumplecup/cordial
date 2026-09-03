//! CLI command bodies. Clap types in `cli` call these after `act` dispatch.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(feature = "elicitation")]
use crate::build_all_active_shadow_deps;
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
use crate::build_workspace_members;
use crate::{
    AddExceptionOutcome, CordialError, CordialResult, CoverageSkipEntry, CrateIr, Disposition,
    ExceptionEntry, NamedRunFilter, Plugin, RunAll, RunFilter, RunOutcome, Session, SessionBuilder,
    StoreLayout, SurrealGraphExport, add_coverage_skip, add_exception, all_plugins,
    backup_exception_files, default_store_home, etiquettes_from_plugins, load_exception_files,
    load_exceptions, lookup_etiquette, render_explain_list, render_explain_page,
    resolve_exceptions_root, run_tracing_instrument_apply,
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
    let artifacts = build_workspace_members(project_root, store, crate_name, force)?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %artifact.rustdoc_json().display(),
            "built rustdoc"
        );
    }
    #[cfg(feature = "elicitation")]
    if let Ok(shadow_dep_artifacts) = build_all_active_shadow_deps(project_root, store, force) {
        for artifact in shadow_dep_artifacts {
            tracing::info!(
                crate_name = artifact.crate_name(),
                via = artifact.reference_member().as_deref().unwrap_or("unknown"),
                path = %artifact.rustdoc_json().display(),
                "built shadow-dep rustdoc"
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
    let artifacts = build_sysroot_libraries(&sysroot, crate_name, force)?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %sysroot.rustdoc_cache_path(artifact.crate_name()).display(),
            "built sysroot rustdoc"
        );
    }
    Ok(())
}

#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn execute_explain(id: Option<&str>) -> CordialResult<()> {
    let plugins = all_plugins();
    let etiquettes = etiquettes_from_plugins(&plugins);
    match id {
        None => {
            write!(io::stdout(), "{}", render_explain_list(&etiquettes))?;
            Ok(())
        }
        Some(query) => {
            let Some(etiquette) = lookup_etiquette(&etiquettes, query) else {
                return Err(CordialError::unknown_etiquette(query));
            };
            write!(io::stdout(), "{}", render_explain_page(etiquette))?;
            Ok(())
        }
    }
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_quality_apply(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    store_home: Option<PathBuf>,
    checklist: Option<&Path>,
    dry_run: bool,
) -> CordialResult<()> {
    #[cfg(not(feature = "crate_attrs"))]
    let _ = store_home;

    #[cfg(feature = "crate_attrs")]
    {
        let home = store_home.clone().unwrap_or_else(default_store_home);
        let summary = crate::run_crate_attrs_apply(project_root, &home, crate_name, dry_run)?;
        tracing::info!(
            inserted_attrs = summary.inserted_attrs,
            changed_files = summary.changed_files,
            already_compliant = summary.skipped_existing,
            unresolved = summary.unresolved,
            "crate-attrs apply"
        );
    }

    #[cfg(feature = "tracing")]
    {
        let checklist_path = checklist
            .map(Path::to_path_buf)
            .unwrap_or_else(|| store.findings_dir().join("tracing-instrument.checklist.md"));
        if checklist_path.is_file() {
            execute_tracing_apply(
                project_root,
                store,
                crate_name,
                Some(&checklist_path),
                dry_run,
            )?;
        } else {
            tracing::warn!(
                path = %checklist_path.display(),
                "tracing apply: no checklist, skipped"
            );
        }
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
    tracing::info!(
        changed_functions = summary.changed_functions,
        changed_files = summary.changed_files,
        already_instrumented = summary.skipped_existing,
        skipped_by_policy = summary.skipped_policy,
        unresolved = summary.unresolved,
        "tracing apply"
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
    tracing::info!(open, suppressed, "findings");
    if !artifacts.is_empty() {
        tracing::info!(?artifacts, "artifacts");
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
pub(super) fn execute_backup_exceptions(
    project_root: &Path,
    store: &StoreLayout,
    root: &Path,
) -> CordialResult<()> {
    let backup_root = resolve_exceptions_root(project_root, root);
    let copied = backup_exception_files(store, &backup_root)?;
    tracing::info!(
        copied,
        path = %backup_root.join(&store.project_slug).display(),
        "backed up exception files"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_load_exceptions(
    project_root: &Path,
    store: &StoreLayout,
    root: &Path,
) -> CordialResult<()> {
    let backup_root = resolve_exceptions_root(project_root, root);
    let copied = load_exception_files(store, &backup_root)?;
    tracing::info!(
        copied,
        path = %backup_root.join(&store.project_slug).display(),
        "loaded exception files"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, entry), err(level = "warn"))]
pub(super) fn execute_add_exception(
    store: &StoreLayout,
    etiquette: &str,
    crate_name: &str,
    entry: ExceptionEntry,
) -> CordialResult<()> {
    print_add_outcome(add_exception(store, etiquette, crate_name, entry)?)
}

#[instrument(level = "debug", skip(store, entry), err(level = "warn"))]
pub(super) fn execute_add_coverage_skip(
    store: &StoreLayout,
    patch_set: &str,
    entry: CoverageSkipEntry,
) -> CordialResult<()> {
    print_add_outcome(add_coverage_skip(store, patch_set, entry)?)
}

#[instrument(level = "debug", skip(outcome))]
fn print_add_outcome(outcome: AddExceptionOutcome) -> CordialResult<()> {
    let verb = if outcome.inserted() {
        "added"
    } else {
        "already present"
    };
    tracing::info!(verb, path = %outcome.path().display(), "exception row");
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn list_exceptions(store: &StoreLayout) -> CordialResult<()> {
    let mut files = Vec::new();
    let exceptions_dir = store.exceptions_dir();
    if exceptions_dir.is_dir() {
        collect_files(&exceptions_dir, &exceptions_dir, &mut files, "exceptions")?;
    }
    let quality_patches_dir = store.quality_patches_dir();
    if quality_patches_dir.is_dir() {
        collect_files(
            &quality_patches_dir,
            &quality_patches_dir,
            &mut files,
            "quality/patches",
        )?;
    }
    let patches_dir = store.patches_dir();
    if patches_dir.is_dir() {
        collect_files(&patches_dir, &patches_dir, &mut files, "patches")?;
    }
    files.sort();
    if files.is_empty() {
        tracing::warn!(
            exceptions = %exceptions_dir.display(),
            quality_patches = %quality_patches_dir.display(),
            patches = %patches_dir.display(),
            "no exception files"
        );
        return Ok(());
    }
    for path in files {
        writeln!(io::stdout(), "{}", path.display())?;
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
        writeln!(io::stdout())?;
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
                .and_then(|targets| targets.into_iter().next().map(|t| t.crate_name().clone()))
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
        tracing::info!(path = %path.display(), "wrote surreal export");
    } else {
        write!(io::stdout(), "{body}")?;
    }
    Ok(())
}
