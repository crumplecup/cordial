//! Leaf native sources for [`super::ScanErrorKind`] -- split out of
//! `scan.rs` once the combined leaf-source + Kind/Error/wrapper file
//! grew past a human reader's single-take size.

use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct ProbeRustdocCacheMissSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ProbeRustdocCacheMissSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for ProbeRustdocCacheMissSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "probe rustdoc cache miss and live probe builds disabled ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for ProbeRustdocCacheMissSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoStdFamilyRustdocCacheSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoStdFamilyRustdocCacheSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoStdFamilyRustdocCacheSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no std-family rustdoc cache found under ~/.cordial/sysroot — run `cordial build sysroot` ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NoStdFamilyRustdocCacheSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CrateIrMustExistSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CrateIrMustExistSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CrateIrMustExistSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "crate ir must exist ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for CrateIrMustExistSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct WorkspaceMissingCrateTargetsSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WorkspaceMissingCrateTargetsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WorkspaceMissingCrateTargetsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "workspace missing crate targets ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for WorkspaceMissingCrateTargetsSource {}

/// `amenable ext`'s own reporter rendered without assessor findings.
/// Split from a single shared `reporter: String` source: each reporter
/// is a materially different consumer, not one generic condition with
/// 3 differently-labeled calls.
#[derive(Debug, derive_getters::Getters)]
pub struct AmenableExtReporterRequiresAssessorFindingsSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AmenableExtReporterRequiresAssessorFindingsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for AmenableExtReporterRequiresAssessorFindingsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "amenable ext reporter requires assessor findings ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for AmenableExtReporterRequiresAssessorFindingsSource {}

/// `amenable std`'s own reporter rendered without assessor findings.
#[derive(Debug, derive_getters::Getters)]
pub struct AmenableStdReporterRequiresAssessorFindingsSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AmenableStdReporterRequiresAssessorFindingsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for AmenableStdReporterRequiresAssessorFindingsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "amenable std reporter requires assessor findings ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for AmenableStdReporterRequiresAssessorFindingsSource {}

/// `homecoming std`'s own reporter rendered without assessor findings.
#[derive(Debug, derive_getters::Getters)]
pub struct HomecomingStdReporterRequiresAssessorFindingsSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl HomecomingStdReporterRequiresAssessorFindingsSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for HomecomingStdReporterRequiresAssessorFindingsSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "homecoming std reporter requires assessor findings ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for HomecomingStdReporterRequiresAssessorFindingsSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct DependencyFreshnessCheckFailedSource {
    #[getter(skip)]
    text: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl DependencyFreshnessCheckFailedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(text))]
    pub fn new(text: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            text: text.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for DependencyFreshnessCheckFailedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "`cargo update --dry-run --verbose` failed while checking dependency freshness: {} ({}:{})",
            self.text, self.file, self.line
        )
    }
}

impl std::error::Error for DependencyFreshnessCheckFailedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct RustdocJsonNotFoundForCrateSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    path: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl RustdocJsonNotFoundForCrateSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, path))]
    pub fn new(crate_name: impl Into<String>, path: PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            path,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for RustdocJsonNotFoundForCrateSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "rustdoc JSON not found for `{}` at {} ({}:{})",
            self.crate_name,
            self.path.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for RustdocJsonNotFoundForCrateSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CrateIrNotLoadedForShadowCompareSource {
    #[getter(skip)]
    crate_name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CrateIrNotLoadedForShadowCompareSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn new(crate_name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CrateIrNotLoadedForShadowCompareSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "crate `{}` IR not loaded in workspace for shadow compare ({}:{})",
            self.crate_name, self.file, self.line
        )
    }
}

impl std::error::Error for CrateIrNotLoadedForShadowCompareSource {}
