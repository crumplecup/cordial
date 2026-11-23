//! The state a project starts from when it has none of its own.
//!
//! Loading a project cold builds the toolchain's sysroot first: its crates' definition maps and the macros
//! they expand, which are the same for every project, and are most of what a project with little code of its
//! own saves. A base is a state saved once from a small workspace that is only that sysroot and a crate that
//! exercises it. A project with no state of its own restores it and loads the project onto it, as it would its
//! own last state, and builds none of the sysroot again (see [`crate::SavedState::with_base`]).
//!
//! It is built the first time a store needs it, in a process of its own (so that what building it holds and
//! says is not the run's), under the shared sysroot cache's lock, in a directory beside the state it is made
//! in, and moved into place when it is whole. It is keyed by the build of the tool that made it, as every
//! state is; a rebuild of the tool builds another and leaves the old one for `cordial clean --all`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::{CordialError, CordialResult, SysrootCache};

/// Set to `on` to start a project that has no state of its own from the store's base state, made when it
/// is first wanted. Until it has been measured on a large project it is not the default: a run with
/// nothing saved loads cold.
pub const BASE_VARIABLE: &str = "CORDIAL_BASE";

/// Changes when what the base is saved from changes, so that a base made of the old workload is not used.
const WORKLOAD_VERSION: u32 = 1;

/// What the base is saved from: the parts of the standard library most crates use, so that what a first
/// method call into them builds (the impls of the traits in play, the signatures of the types) is in it.
const WORKLOAD: &str = "\
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Debug, Display};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Record {
    pub name: String,
    pub values: Vec<u32>,
}

pub trait Summary {
    fn summary(&self) -> String;
}

impl Summary for Record {
    fn summary(&self) -> String {
        format!(\"{}: {}\", self.name, self.values.iter().map(|value| value.to_string()).collect::<Vec<_>>().join(\",\"))
    }
}

impl Display for Record {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, \"{}\", self.summary())
    }
}

pub fn group(records: &[Record]) -> HashMap<String, Vec<u32>> {
    let mut grouped: HashMap<String, Vec<u32>> = HashMap::new();
    for record in records {
        grouped.entry(record.name.clone()).or_default().extend(record.values.iter().copied());
    }
    grouped
}

pub fn ordered(records: &[Record]) -> BTreeMap<String, usize> {
    records.iter().map(|record| (record.name.clone(), record.values.len())).collect()
}

pub fn unique(records: &[Record]) -> HashSet<Record> {
    records.iter().cloned().collect()
}

pub fn largest<T: Ord + Clone>(items: &[T]) -> Option<T> {
    items.iter().max().cloned()
}

pub fn parse(text: &str) -> Result<Vec<u32>, std::num::ParseIntError> {
    text.split(',').map(|part| part.trim().parse::<u32>()).collect()
}

pub fn describe<T: Debug>(value: &T) -> String {
    format!(\"{value:?}\")
}

pub fn run(records: Vec<Record>) -> Result<usize, Box<dyn std::error::Error>> {
    let total: usize = records.iter().filter(|record| !record.values.is_empty()).map(|record| record.values.len()).sum();
    let _ = parse(\"1, 2, 3\")?;
    Ok(total)
}
";

/// The key of the base this build of the tool makes and uses: its executable and the workload's version.
#[instrument(level = "trace")]
pub fn key() -> String {
    let digest = Sha256::digest(
        format!(
            "{}:{WORKLOAD_VERSION}",
            crate::run_cache::executable_fingerprint()
        )
        .as_bytes(),
    );
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Where this build's base is, whether or not it has been made yet.
#[instrument(level = "trace", skip(sysroot))]
pub fn directory(sysroot: &SysrootCache) -> PathBuf {
    sysroot.cache_dir().join("base-state").join(key())
}

/// Whether the base in `directory` was finished: a state is moved into place with its key last.
#[must_use]
pub fn is_ready(directory: &Path) -> bool {
    directory.join("key").is_file()
}

/// Whether the environment asks for a base.
#[must_use]
pub fn enabled() -> bool {
    std::env::var_os(BASE_VARIABLE).is_some_and(|value| value == "on")
}

/// The base for this build of the tool, made if there is none: `None` when none is asked for or it could
/// not be made, which is a run that loads cold and says why.
#[instrument(level = "debug", skip(sysroot))]
pub fn ensure(sysroot: &SysrootCache) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let directory = directory(sysroot);
    if is_ready(&directory) {
        return Some(directory);
    }
    crate::host::event("base state: none yet for this build; making it".to_owned());
    let started = std::time::Instant::now();
    match build_in_a_process(sysroot) {
        Ok(()) if is_ready(&directory) => {
            crate::host::event(format!("base state: made in {:?}", started.elapsed()));
            Some(directory)
        }
        Ok(()) => {
            crate::host::event("base state: not made; loading cold".to_owned());
            None
        }
        Err(error) => {
            crate::host::event(format!("base state: not made ({error}); loading cold"));
            None
        }
    }
}

/// Run `cordial base build` as a process of its own and wait for it.
fn build_in_a_process(sysroot: &SysrootCache) -> CordialResult<()> {
    let home = sysroot
        .root()
        .parent()
        .unwrap_or_else(|| sysroot.root())
        .to_path_buf();
    let status = std::process::Command::new(std::env::current_exe()?)
        .arg("--store-home")
        .arg(home)
        .args(["base", "build"])
        .env(BASE_VARIABLE, "on")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CordialError::perf(
            sysroot.root().clone(),
            format!("building the base state ended with {status}"),
        ))
    }
}

/// Make this build's base if it is not there: the workload in a project of its own, loaded and
/// projected once so that the sysroot is built and saved, and the state moved into place.
#[instrument(level = "info", skip(sysroot), err(level = "warn"))]
pub fn build(sysroot: &SysrootCache) -> CordialResult<PathBuf> {
    let directory = directory(sysroot);
    let _lock = crate::SysrootLock::exclusive(sysroot, "base build", crate::noop_progress())?;
    if is_ready(&directory) {
        // Another process made it while this one waited for the lock.
        return Ok(directory);
    }
    let parent = directory
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| sysroot.cache_dir());
    std::fs::create_dir_all(&parent)?;
    let building = parent.join(format!("{}.building-{}", key(), std::process::id()));
    let _ = std::fs::remove_dir_all(&building);
    let project = building.join("project");
    std::fs::create_dir_all(project.join("src"))?;
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"cordial_base\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    std::fs::write(project.join("src/lib.rs"), WORKLOAD)?;

    let state = crate::SavedState::at(building.join("state"));
    let mut session = crate::SourceDatabase::load_saved(
        &project,
        &crate::SemanticCargoOptions::default(),
        Some(state.clone()),
    )?;
    session.project(
        &crate::CrateTarget::new("cordial_base", &project),
        &[project.join("src/lib.rs")],
    )?;
    drop(session);

    let _ = std::fs::remove_dir_all(&directory);
    std::fs::rename(state.directory(), &directory)?;
    let _ = std::fs::remove_dir_all(&building);
    Ok(directory)
}
