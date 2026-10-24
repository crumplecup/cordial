//! Native sources for resolving and invoking the nightly toolchain
//! needed for std-family sysroot rustdoc JSON.

use std::panic::Location;
use std::path::PathBuf;
use std::process::ExitStatus;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct RustcVvExitedSource {
    #[getter(copy)]
    status: ExitStatus,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl RustcVvExitedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(status))]
    pub fn new(status: ExitStatus) -> Self {
        let loc = Location::caller();
        Self {
            status,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for RustcVvExitedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "rustc -vV exited with {} ({}:{})",
            self.status, self.file, self.line
        )
    }
}

impl std::error::Error for RustcVvExitedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct RustcVvMissingHostTargetSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl RustcVvMissingHostTargetSource {
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

impl std::fmt::Display for RustcVvMissingHostTargetSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "rustc -vV output missing host target triple ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for RustcVvMissingHostTargetSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NightlyCargoMissingRustcSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NightlyCargoMissingRustcSource {
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

impl std::fmt::Display for NightlyCargoMissingRustcSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "nightly toolchain cargo found but rustc binary missing ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NightlyCargoMissingRustcSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NightlyToolchainRequiredSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NightlyToolchainRequiredSource {
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

impl std::fmt::Display for NightlyToolchainRequiredSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "nightly toolchain required for std-family rustdoc JSON (install via rustup) ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NightlyToolchainRequiredSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NightlyToolchainRootUnresolvedSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NightlyToolchainRootUnresolvedSource {
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

impl std::fmt::Display for NightlyToolchainRootUnresolvedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "could not resolve nightly toolchain root from cargo binary path ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NightlyToolchainRootUnresolvedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SysrootManifestNotFoundSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    manifest: PathBuf,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SysrootManifestNotFoundSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, manifest))]
    pub fn new(crate_name: impl Into<String>, manifest: PathBuf) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            manifest,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SysrootManifestNotFoundSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "sysroot manifest for `{}` not found at {} ({}:{})",
            self.crate_name,
            self.manifest.display(),
            self.file,
            self.line
        )
    }
}

impl std::error::Error for SysrootManifestNotFoundSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SysrootBuildRequestedForNonStdFamilyCrateSource {
    #[getter(skip)]
    name: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SysrootBuildRequestedForNonStdFamilyCrateSource {
    #[track_caller]
    #[instrument(level = "debug", skip(name))]
    pub fn new(name: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            name: name.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SysrootBuildRequestedForNonStdFamilyCrateSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "sysroot build requested for non-std-family crate `{}` ({}:{})",
            self.name, self.file, self.line
        )
    }
}

impl std::error::Error for SysrootBuildRequestedForNonStdFamilyCrateSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SysrootCargoRustdocExitedSource {
    #[getter(skip)]
    crate_name: String,
    #[getter(copy)]
    status: ExitStatus,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SysrootCargoRustdocExitedSource {
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, status))]
    pub fn new(crate_name: impl Into<String>, status: ExitStatus) -> Self {
        let loc = Location::caller();
        Self {
            crate_name: crate_name.into(),
            status,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SysrootCargoRustdocExitedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "sysroot cargo rustdoc for {} exited with {} ({}:{})",
            self.crate_name, self.status, self.file, self.line
        )
    }
}

impl std::error::Error for SysrootCargoRustdocExitedSource {}

/// Umbrella kind for nightly-toolchain resolution/invocation failures,
/// nested one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum CargoRustdocToolchainErrorKind {
    RustcVvExited(RustcVvExitedSource),
    RustcVvMissingHostTarget(RustcVvMissingHostTargetSource),
    NightlyCargoMissingRustc(NightlyCargoMissingRustcSource),
    NightlyToolchainRequired(NightlyToolchainRequiredSource),
    NightlyToolchainRootUnresolved(NightlyToolchainRootUnresolvedSource),
    SysrootManifestNotFound(SysrootManifestNotFoundSource),
    SysrootBuildRequestedForNonStdFamilyCrate(SysrootBuildRequestedForNonStdFamilyCrateSource),
    SysrootCargoRustdocExited(SysrootCargoRustdocExitedSource),
}

#[derive(Debug)]
pub struct CargoRustdocToolchainError {
    kind: Box<CargoRustdocToolchainErrorKind>,
    file: String,
    line: u32,
}

impl CargoRustdocToolchainError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: CargoRustdocToolchainErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for CargoRustdocToolchainError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            CargoRustdocToolchainErrorKind::RustcVvExited(source) => source.fmt(formatter),
            CargoRustdocToolchainErrorKind::RustcVvMissingHostTarget(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::NightlyCargoMissingRustc(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::NightlyToolchainRequired(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::NightlyToolchainRootUnresolved(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::SysrootManifestNotFound(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::SysrootBuildRequestedForNonStdFamilyCrate(source) => {
                source.fmt(formatter)
            }
            CargoRustdocToolchainErrorKind::SysrootCargoRustdocExited(source) => {
                source.fmt(formatter)
            }
        }
    }
}

impl std::error::Error for CargoRustdocToolchainError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            CargoRustdocToolchainErrorKind::RustcVvExited(source) => Some(source),
            CargoRustdocToolchainErrorKind::RustcVvMissingHostTarget(source) => Some(source),
            CargoRustdocToolchainErrorKind::NightlyCargoMissingRustc(source) => Some(source),
            CargoRustdocToolchainErrorKind::NightlyToolchainRequired(source) => Some(source),
            CargoRustdocToolchainErrorKind::NightlyToolchainRootUnresolved(source) => Some(source),
            CargoRustdocToolchainErrorKind::SysrootManifestNotFound(source) => Some(source),
            CargoRustdocToolchainErrorKind::SysrootBuildRequestedForNonStdFamilyCrate(source) => {
                Some(source)
            }
            CargoRustdocToolchainErrorKind::SysrootCargoRustdocExited(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `rustc -vV` exited with a non-zero status.
    #[track_caller]
    #[instrument(level = "debug", skip(status))]
    pub fn rustc_vv_exited(status: ExitStatus) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(CargoRustdocToolchainErrorKind::RustcVvExited(
                RustcVvExitedSource::new(status),
            )),
        ))
    }

    /// `rustc -vV` output had no `host:` line.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn rustc_vv_missing_host_target() -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::RustcVvMissingHostTarget(
                    RustcVvMissingHostTargetSource::new(),
                ),
            ),
        ))
    }

    /// A resolved nightly `cargo` binary has no sibling `rustc`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn nightly_cargo_missing_rustc() -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::NightlyCargoMissingRustc(
                    NightlyCargoMissingRustcSource::new(),
                ),
            ),
        ))
    }

    /// No nightly toolchain is available for std-family rustdoc JSON.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn nightly_toolchain_required() -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::NightlyToolchainRequired(
                    NightlyToolchainRequiredSource::new(),
                ),
            ),
        ))
    }

    /// The nightly toolchain root could not be resolved from the cargo binary path.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn nightly_toolchain_root_unresolved() -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::NightlyToolchainRootUnresolved(
                    NightlyToolchainRootUnresolvedSource::new(),
                ),
            ),
        ))
    }

    /// A std-family sysroot library's manifest was not found.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, manifest))]
    pub fn sysroot_manifest_not_found(crate_name: impl Into<String>, manifest: PathBuf) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::SysrootManifestNotFound(
                    SysrootManifestNotFoundSource::new(crate_name, manifest),
                ),
            ),
        ))
    }

    /// A sysroot build was requested for a crate that is not std-family.
    #[track_caller]
    #[instrument(level = "debug", skip(name))]
    pub fn sysroot_build_requested_for_non_std_family_crate(name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::SysrootBuildRequestedForNonStdFamilyCrate(
                    SysrootBuildRequestedForNonStdFamilyCrateSource::new(name),
                ),
            ),
        ))
    }

    /// `cargo rustdoc` for a sysroot library exited with a non-zero status.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name, status))]
    pub fn sysroot_cargo_rustdoc_exited(crate_name: impl Into<String>, status: ExitStatus) -> Self {
        Self::from_kind(super::CordialErrorKind::CargoRustdocToolchain(
            CargoRustdocToolchainError::new(
                CargoRustdocToolchainErrorKind::SysrootCargoRustdocExited(
                    SysrootCargoRustdocExitedSource::new(crate_name, status),
                ),
            ),
        ))
    }
}
