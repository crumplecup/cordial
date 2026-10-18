//! Output and dispatch for `cordial clean`.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::Path;

use tracing::instrument;

use crate::{
    CleanPlan, CleanScope, CordialError, CordialResult, ProgressSink, StoreLayout, SysrootCache,
    SysrootLock, WorklistLock, default_store_home, plan_clean, project_slug_from_path,
    run_clean_with_progress,
};

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
pub(super) fn execute_clean(
    project_root: &Path,
    store: &StoreLayout,
    store_home: Option<&Path>,
    progress: &dyn ProgressSink,
    all: bool,
    dry_run: bool,
) -> CordialResult<()> {
    let scope = if all {
        CleanScope::All {
            store_home: store_home.map_or_else(default_store_home, Path::to_path_buf),
            project_root: project_root.to_path_buf(),
        }
    } else {
        CleanScope::Project {
            store: store.clone(),
            project_root: project_root.to_path_buf(),
        }
    };
    let task = progress.spinner("Planning clean".into());
    let initial = plan_clean(scope.clone())?;
    let mut project_locks = Vec::new();
    let mut sysroot_lock = None;
    let plan = if all {
        let sysroot_root =
            SysrootCache::from_home(store_home.map_or_else(default_store_home, Path::to_path_buf));
        let mut held = BTreeSet::new();
        let mut busy = BTreeSet::new();
        for root in initial.stores() {
            if root == sysroot_root.root() {
                if let Some(lock) = SysrootLock::try_exclusive(&sysroot_root, "clean --all")? {
                    sysroot_lock = Some(lock);
                    held.insert(root.clone());
                } else {
                    busy.insert(root.clone());
                }
            } else {
                let store = StoreLayout::from_root(root, project_slug_from_path(root));
                if let Some(lock) = WorklistLock::try_exclusive(&store, "clean --all")? {
                    project_locks.push(lock);
                    held.insert(root.clone());
                } else {
                    busy.insert(root.clone());
                }
            }
        }
        let mut plan = plan_clean(scope)?;
        for root in plan.stores().iter().filter(|root| !held.contains(*root)) {
            busy.insert(root.clone());
        }
        for root in busy {
            plan.exclude_store(&root, "busy or changed during planning");
        }
        plan
    } else {
        if let Some(skipped) = initial.skipped().first() {
            return Err(CordialError::clean(
                skipped.path().clone(),
                skipped.reason(),
            ));
        }
        if !initial.stores().is_empty() {
            project_locks.push(WorklistLock::exclusive(store, "clean", progress)?);
            plan_clean(scope)?
        } else {
            initial
        }
    };
    let _locks = (project_locks, sysroot_lock);
    task.finish(format!("Planned {} path(s)", plan.paths().len()));
    print_plan(&plan, dry_run)?;
    if !all {
        if let Some(skipped) = plan.skipped().first() {
            return Err(CordialError::clean(
                skipped.path().clone(),
                skipped.reason(),
            ));
        }
    }
    if dry_run {
        let skipped = plan
            .skipped()
            .iter()
            .find(|item| !item.reason().starts_with("unrecognized"));
        if let Some(skipped) = skipped {
            return Err(CordialError::clean(
                skipped.path().clone(),
                skipped.reason(),
            ));
        }
        return Ok(());
    }
    let report = run_clean_with_progress(plan, progress);
    let mut output = io::stdout().lock();
    if report.removed().is_empty() {
        writeln!(output, "Nothing was removed.")?;
    } else {
        writeln!(output, "Freed {}.", format_bytes(report.freed_bytes()))?;
        writeln!(output, "The next run rebuilds the cache and may be slower.")?;
    }
    if report.checked_off() > 0 {
        writeln!(
            output,
            "Discarded {} checked-off worklist item(s).",
            report.checked_off()
        )?;
    }
    for failure in report.failures() {
        writeln!(
            output,
            "Failed: {}: {}",
            failure.path().display(),
            failure.error()
        )?;
    }
    if !report.complete() {
        return Err(CordialError::clean(
            store.root().clone(),
            "clean incomplete; see skipped stores and failed paths",
        ));
    }
    Ok(())
}

#[instrument(level = "debug", skip(plan), err(level = "warn"))]
fn print_plan(plan: &CleanPlan, dry_run: bool) -> CordialResult<()> {
    let mut output = io::stdout().lock();
    if plan.paths().is_empty() {
        writeln!(output, "Nothing to clean.")?;
    } else {
        writeln!(
            output,
            "{}:",
            if dry_run { "Would remove" } else { "Removing" }
        )?;
        for path in plan.paths() {
            writeln!(
                output,
                "  {}  {}",
                path.path().display(),
                format_bytes(path.bytes())
            )?;
        }
        writeln!(
            output,
            "{} {} across {} path(s).",
            if dry_run { "Would free" } else { "Planned" },
            format_bytes(plan.total_bytes()),
            plan.paths().len()
        )?;
    }
    for skipped in plan.skipped() {
        writeln!(
            output,
            "Skipped {}: {}",
            skipped.path().display(),
            skipped.reason()
        )?;
    }
    if dry_run {
        writeln!(output, "Nothing was removed.")?;
    }
    Ok(())
}

#[instrument(level = "trace")]
fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    if bytes >= KIB * KIB * KIB {
        format!("{:.1} GiB", bytes as f64 / (KIB * KIB * KIB) as f64)
    } else if bytes >= KIB * KIB {
        format!("{:.1} MiB", bytes as f64 / (KIB * KIB) as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
