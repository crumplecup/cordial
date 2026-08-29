use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::error::CordialResult;

/// Local store layout under `~/.cordial/{project}/`.
#[derive(Debug, Clone)]
pub struct StoreLayout {
    /// Directory-name slug identifying this project in the store.
    pub project_slug: String,
    /// Filesystem or graph root.
    pub root: PathBuf,
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
        Ok(())
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
#[derive(Debug, Clone)]
pub struct SysrootCache {
    /// Filesystem or graph root.
    pub root: PathBuf,
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

    /// Create cache, findings, and exceptions directories if missing.
    #[instrument(level = "info", skip(self), err(level = "warn"))]
    pub fn ensure_dirs(&self) -> CordialResult<()> {
        std::fs::create_dir_all(self.rustdoc_cache_dir())?;
        std::fs::create_dir_all(self.builds_dir())?;
        Ok(())
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
