use std::path::{Path, PathBuf};
use std::{fs::OpenOptions, io::ErrorKind};

use tracing::instrument;

use crate::error::CordialResult;

/// Local store layout under `~/.cordial/{project}/`.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct StoreLayout {
    /// Directory-name slug identifying this project in the store.
    project_slug: String,
    /// Filesystem or graph root.
    root: PathBuf,
}

impl StoreLayout {
    /// Construct a new value.
    #[instrument(level = "debug", skip(project_slug), ret)]
    pub fn new(project_slug: impl Into<String>) -> Self {
        let project_slug = project_slug.into();
        let root = default_store_home().join(&project_slug);
        Self { project_slug, root }
    }

    /// Build a store layout rooted at `root`.
    #[instrument(level = "debug", skip(root, project_slug), ret)]
    pub fn from_root(root: impl Into<PathBuf>, project_slug: impl Into<String>) -> Self {
        Self {
            project_slug: project_slug.into(),
            root: root.into(),
        }
    }

    /// Create cache, findings, and exceptions directories if missing.
    #[instrument(level = "info", skip(self), err(level = "warn"))]
    pub fn ensure_dirs(&self) -> CordialResult<()> {
        std::fs::create_dir_all(self.cache_dir())?;
        std::fs::create_dir_all(self.findings_dir())?;
        std::fs::create_dir_all(self.exceptions_dir())?;
        write_marker(&self.marker_path())?;
        Ok(())
    }

    /// Marker identifying a project store, including after its cache is cleaned.
    #[instrument(level = "trace", skip(self))]
    pub fn marker_path(&self) -> PathBuf {
        self.root.join(".cordial-store")
    }

    /// Paths that can be rebuilt by another Cordial run.
    #[instrument(level = "trace", skip(self))]
    pub fn regenerable_paths(&self) -> Vec<PathBuf> {
        vec![self.cache_dir(), self.findings_dir(), self.worklist_path()]
    }

    /// Paths that contain user-maintained data and must survive cleaning.
    #[instrument(level = "trace", skip(self))]
    pub fn curated_paths(&self) -> Vec<PathBuf> {
        vec![
            self.exceptions_dir(),
            self.quality_patches_dir(),
            self.patches_dir(),
            self.config_dir(),
        ]
    }

    /// Directory for cached IR and rustdoc artifacts.
    #[instrument(level = "trace", skip(self))]
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Directory for generated reports.
    #[instrument(level = "trace", skip(self))]
    pub fn findings_dir(&self) -> PathBuf {
        self.root.join("findings")
    }

    /// Persistent agent worklist beside generated findings.
    #[instrument(level = "trace", skip(self))]
    pub fn worklist_path(&self) -> PathBuf {
        self.root.join("worklist.json")
    }

    /// Advisory lock file for worklist reads and updates.
    #[instrument(level = "trace", skip(self))]
    pub fn worklist_lock_path(&self) -> PathBuf {
        self.root.join("worklist.lock")
    }

    /// Directory for JSON patch exception files.
    #[instrument(level = "trace", skip(self))]
    pub fn exceptions_dir(&self) -> PathBuf {
        self.root.join("exceptions")
    }

    /// Per-etiquette JSON config: `{store}/config/{etiquette}.json`.
    #[instrument(level = "trace", skip(self))]
    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    /// Etiquette config path.
    #[instrument(level = "trace", skip(self))]
    pub fn etiquette_config_path(&self, etiquette_id: &str) -> PathBuf {
        self.config_dir().join(format!("{etiquette_id}.json"))
    }

    /// Patches dir.
    #[instrument(level = "trace", skip(self))]
    pub fn patches_dir(&self) -> PathBuf {
        self.root.join("patches")
    }

    /// elicit_doc-compatible patch layout: `{store}/quality/patches/{etiquette}/{crate}.json`.
    #[instrument(level = "trace", skip(self))]
    pub fn quality_patches_dir(&self) -> PathBuf {
        self.root.join("quality").join("patches")
    }

    /// Directory for rustdoc build artifacts.
    #[instrument(level = "trace", skip(self))]
    pub fn builds_dir(&self) -> PathBuf {
        self.cache_dir().join("builds")
    }

    /// Directory for cached rustdoc JSON.
    #[instrument(level = "trace", skip(self))]
    pub fn rustdoc_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("rustdoc")
    }

    /// Cargo `--target-dir` for rustdoc JSON builds (`cache/rustdoc-target`).
    #[instrument(level = "trace", skip(self))]
    pub fn rustdoc_target_dir(&self) -> PathBuf {
        self.cache_dir().join("rustdoc-target")
    }

    /// Path of the cached rustdoc build artifact for this crate.
    #[instrument(level = "trace", skip(self))]
    pub fn build_artifact_path(&self, crate_name: &str) -> PathBuf {
        self.builds_dir().join(format!("{crate_name}.build.json"))
    }

    /// Path of cached rustdoc JSON for this crate.
    #[instrument(level = "trace", skip(self))]
    pub fn rustdoc_cache_path(&self, crate_name: &str) -> PathBuf {
        self.rustdoc_cache_dir().join(format!("{crate_name}.json"))
    }

    /// Stable cache stem for upstream rustdoc built through a shadow member dependency edge.
    #[instrument(level = "debug")]
    pub fn shadow_dep_cache_stem(shadow_crate: &str, upstream_crate: &str) -> String {
        format!("shadow-dep-{shadow_crate}-{upstream_crate}")
    }

    /// Shadow dep rustdoc cache path.
    #[instrument(level = "trace", skip(self))]
    pub fn shadow_dep_rustdoc_cache_path(
        &self,
        shadow_crate: &str,
        upstream_crate: &str,
    ) -> PathBuf {
        self.rustdoc_cache_dir().join(format!(
            "{}.json",
            Self::shadow_dep_cache_stem(shadow_crate, upstream_crate)
        ))
    }

    /// Shadow dep build artifact path.
    #[instrument(level = "trace", skip(self))]
    pub fn shadow_dep_build_artifact_path(
        &self,
        shadow_crate: &str,
        upstream_crate: &str,
    ) -> PathBuf {
        self.builds_dir().join(format!(
            "{}.build.json",
            Self::shadow_dep_cache_stem(shadow_crate, upstream_crate)
        ))
    }

    /// Ir cache path.
    #[instrument(level = "trace", skip(self))]
    pub fn ir_cache_path(&self, crate_name: &str) -> PathBuf {
        crate::ir::CrateIr::cache_path(&self.cache_dir(), crate_name)
    }
}

/// Shared sysroot rustdoc cache under `{CORDIAL_HOME}/sysroot/`.
///
/// Std-family inventories (`std`, `core`, `alloc`) are toolchain-global and live
/// here rather than in per-project stores.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct SysrootCache {
    /// Filesystem or graph root.
    root: PathBuf,
}

impl SysrootCache {
    /// Default location: `~/.cordial/sysroot` (or `$CORDIAL_HOME/sysroot`).
    #[instrument(level = "debug")]
    pub fn default_cache() -> Self {
        Self::from_home(default_store_home())
    }

    /// Place the sysroot cache under `{home}/sysroot`.
    #[instrument(level = "debug", skip(home), ret)]
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        Self {
            root: home.into().join("sysroot"),
        }
    }

    /// Build a store layout rooted at `root`.
    #[instrument(level = "debug", skip(root), ret)]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create the shared cache directories and identifying marker if missing.
    #[instrument(level = "info", skip(self), err(level = "warn"))]
    pub fn ensure_dirs(&self) -> CordialResult<()> {
        std::fs::create_dir_all(self.rustdoc_cache_dir())?;
        std::fs::create_dir_all(self.builds_dir())?;
        write_marker(&self.marker_path())?;
        Ok(())
    }

    /// Marker identifying the shared sysroot cache.
    #[instrument(level = "trace", skip(self))]
    pub fn marker_path(&self) -> PathBuf {
        self.root.join(".cordial-sysroot")
    }

    /// Shared artifacts that can be rebuilt for the installed toolchain.
    #[instrument(level = "trace", skip(self))]
    pub fn regenerable_paths(&self) -> Vec<PathBuf> {
        vec![self.cache_dir()]
    }

    /// Advisory lock file for shared sysroot cache writes and cleaning.
    #[instrument(level = "trace", skip(self))]
    pub fn lock_path(&self) -> PathBuf {
        self.root.join("sysroot.lock")
    }

    /// Directory for cached IR and rustdoc artifacts.
    #[instrument(level = "trace", skip(self))]
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Directory for cached rustdoc JSON.
    #[instrument(level = "trace", skip(self))]
    pub fn rustdoc_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("rustdoc")
    }

    /// Directory for rustdoc build artifacts.
    #[instrument(level = "trace", skip(self))]
    pub fn builds_dir(&self) -> PathBuf {
        self.cache_dir().join("builds")
    }

    /// Build target dir.
    #[instrument(level = "trace", skip(self))]
    pub fn build_target_dir(&self) -> PathBuf {
        self.cache_dir().join("target")
    }

    /// Path of cached rustdoc JSON for this crate.
    #[instrument(level = "trace", skip(self))]
    pub fn rustdoc_cache_path(&self, crate_name: &str) -> PathBuf {
        self.rustdoc_cache_dir().join(format!("{crate_name}.json"))
    }

    /// Path of the cached rustdoc build artifact for this crate.
    #[instrument(level = "trace", skip(self))]
    pub fn build_artifact_path(&self, crate_name: &str) -> PathBuf {
        self.builds_dir().join(format!("{crate_name}.build.json"))
    }
}

impl Default for SysrootCache {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self::default_cache()
    }
}

#[instrument(level = "info", skip(path), err(level = "warn"))]
fn write_marker(path: &Path) -> CordialResult<()> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if path.symlink_metadata()?.file_type().is_file() {
                Ok(())
            } else {
                Err(error.into())
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Default store home.
#[instrument(level = "debug")]
pub fn default_store_home() -> PathBuf {
    std::env::var_os("CORDIAL_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cordial")))
        .unwrap_or_else(|| PathBuf::from(".cordial"))
}

/// Project slug from path.
#[instrument(level = "debug")]
pub fn project_slug_from_path(project_root: &Path) -> String {
    project_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string()
}
