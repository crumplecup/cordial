//! Kind/Error/wrapper-constructor machinery for the remaining
//! scan-engine internal invariants: probe/rustdoc-cache lookups,
//! reporter preconditions, and dependency freshness. The leaf native
//! sources live in `scan_sources.rs`.

use std::panic::Location;
use std::path::PathBuf;

use tracing::instrument;

use super::scan_sources::{
    AmenableExtReporterRequiresAssessorFindingsSource,
    AmenableStdReporterRequiresAssessorFindingsSource, CrateIrMustExistSource,
    CrateIrNotLoadedForShadowCompareSource, DependencyFreshnessCheckFailedSource,
    HomecomingStdReporterRequiresAssessorFindingsSource, NoStdFamilyRustdocCacheSource,
    ProbeRustdocCacheMissSource, RustdocJsonNotFoundForCrateSource,
    WorkspaceMissingCrateTargetsSource,
};

/// Umbrella kind for the remaining scan-engine invariants (probe/rustdoc
/// caches, reporter preconditions, dependency freshness), nested one
/// level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum ScanErrorKind {
    ProbeRustdocCacheMiss(ProbeRustdocCacheMissSource),
    NoStdFamilyRustdocCache(NoStdFamilyRustdocCacheSource),
    CrateIrMustExist(CrateIrMustExistSource),
    WorkspaceMissingCrateTargets(WorkspaceMissingCrateTargetsSource),
    AmenableExtReporterRequiresAssessorFindings(AmenableExtReporterRequiresAssessorFindingsSource),
    AmenableStdReporterRequiresAssessorFindings(AmenableStdReporterRequiresAssessorFindingsSource),
    HomecomingStdReporterRequiresAssessorFindings(
        HomecomingStdReporterRequiresAssessorFindingsSource,
    ),
    DependencyFreshnessCheckFailed(DependencyFreshnessCheckFailedSource),
    RustdocJsonNotFoundForCrate(RustdocJsonNotFoundForCrateSource),
    CrateIrNotLoadedForShadowCompare(CrateIrNotLoadedForShadowCompareSource),
}

#[derive(Debug)]
pub struct ScanError {
    kind: Box<ScanErrorKind>,
    file: String,
    line: u32,
}

impl ScanError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: ScanErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for ScanError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            ScanErrorKind::ProbeRustdocCacheMiss(source) => source.fmt(formatter),
            ScanErrorKind::NoStdFamilyRustdocCache(source) => source.fmt(formatter),
            ScanErrorKind::CrateIrMustExist(source) => source.fmt(formatter),
            ScanErrorKind::WorkspaceMissingCrateTargets(source) => source.fmt(formatter),
            ScanErrorKind::AmenableExtReporterRequiresAssessorFindings(source) => {
                source.fmt(formatter)
            }
            ScanErrorKind::AmenableStdReporterRequiresAssessorFindings(source) => {
                source.fmt(formatter)
            }
            ScanErrorKind::HomecomingStdReporterRequiresAssessorFindings(source) => {
                source.fmt(formatter)
            }
            ScanErrorKind::DependencyFreshnessCheckFailed(source) => source.fmt(formatter),
            ScanErrorKind::RustdocJsonNotFoundForCrate(source) => source.fmt(formatter),
            ScanErrorKind::CrateIrNotLoadedForShadowCompare(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for ScanError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            ScanErrorKind::ProbeRustdocCacheMiss(source) => Some(source),
            ScanErrorKind::NoStdFamilyRustdocCache(source) => Some(source),
            ScanErrorKind::CrateIrMustExist(source) => Some(source),
            ScanErrorKind::WorkspaceMissingCrateTargets(source) => Some(source),
            ScanErrorKind::AmenableExtReporterRequiresAssessorFindings(source) => Some(source),
            ScanErrorKind::AmenableStdReporterRequiresAssessorFindings(source) => Some(source),
            ScanErrorKind::HomecomingStdReporterRequiresAssessorFindings(source) => Some(source),
            ScanErrorKind::DependencyFreshnessCheckFailed(source) => Some(source),
            ScanErrorKind::RustdocJsonNotFoundForCrate(source) => Some(source),
            ScanErrorKind::CrateIrNotLoadedForShadowCompare(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// A probe rustdoc cache was missing and live probe builds are disabled.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn probe_rustdoc_cache_miss() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::ProbeRustdocCacheMiss(ProbeRustdocCacheMissSource::new()),
        )))
    }

    /// `amenable ext`'s reporter rendered without any assessor findings.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn amenable_ext_reporter_requires_assessor_findings() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::AmenableExtReporterRequiresAssessorFindings(
                AmenableExtReporterRequiresAssessorFindingsSource::new(),
            ),
        )))
    }

    /// `amenable std`'s reporter rendered without any assessor findings.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn amenable_std_reporter_requires_assessor_findings() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::AmenableStdReporterRequiresAssessorFindings(
                AmenableStdReporterRequiresAssessorFindingsSource::new(),
            ),
        )))
    }

    /// `homecoming std`'s reporter rendered without any assessor findings.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn homecoming_std_reporter_requires_assessor_findings() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::HomecomingStdReporterRequiresAssessorFindings(
                HomecomingStdReporterRequiresAssessorFindingsSource::new(),
            ),
        )))
    }

    /// `cargo update --dry-run --verbose` failed while checking dependency freshness.
    #[track_caller]
    #[instrument(level = "debug", skip(text))]
    pub fn dependency_freshness_check_failed(text: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::DependencyFreshnessCheckFailed(
                DependencyFreshnessCheckFailedSource::new(text),
            ),
        )))
    }

    /// Rustdoc JSON was not found for a specific crate.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, path))]
    pub fn rustdoc_json_not_found_for_crate(crate_name: impl Into<String>, path: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::RustdocJsonNotFoundForCrate(RustdocJsonNotFoundForCrateSource::new(
                crate_name, path,
            )),
        )))
    }

    /// No std-family rustdoc cache was found under the sysroot store.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn no_std_family_rustdoc_cache() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::NoStdFamilyRustdocCache(NoStdFamilyRustdocCacheSource::new()),
        )))
    }

    /// A crate's IR must exist in the workspace after enrichment.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn crate_ir_must_exist() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::CrateIrMustExist(CrateIrMustExistSource::new()),
        )))
    }

    /// A crate's IR was not loaded in the workspace for a shadow compare.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn crate_ir_not_loaded_for_shadow_compare(crate_name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::CrateIrNotLoadedForShadowCompare(
                CrateIrNotLoadedForShadowCompareSource::new(crate_name),
            ),
        )))
    }

    /// The workspace had no crate targets to render a report for.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn workspace_missing_crate_targets() -> Self {
        Self::from_kind(super::CordialErrorKind::Scan(ScanError::new(
            ScanErrorKind::WorkspaceMissingCrateTargets(WorkspaceMissingCrateTargetsSource::new()),
        )))
    }
}
