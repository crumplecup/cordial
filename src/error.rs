//! Crate error: parent boxes a Kind; Kind variants are native sources.

mod cargo_rustdoc_build;
mod cargo_rustdoc_metadata;
mod cargo_rustdoc_toolchain;
mod clean;
mod exceptions_cli;
mod exceptions_edit;
mod graph;
mod graph_apply_sources;
mod graph_sources;
mod io;
mod lint_order;
mod local;
mod parse;
mod scan;
mod scan_sources;
mod store_lock;
mod targets;
mod tier;
mod time;
#[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
mod toml_parse;
mod worklist_cli;
mod worklist_state;

use std::fmt::{Display, Formatter, Result as FmtResult};

use cargo_rustdoc_build::CargoRustdocBuildError;
use cargo_rustdoc_metadata::CargoRustdocMetadataError;
use cargo_rustdoc_toolchain::CargoRustdocToolchainError;
use clean::CleanSource;
use exceptions_cli::ExceptionsCliError;
use exceptions_edit::ExceptionsEditError;
use graph::GraphError;
use io::{FmtSource, IoSource, PrefixSource};
use lint_order::{LintOrderCycleSource, LintOrderDanglingSource, LintOrderUnknownConstraintSource};
use local::{
    ApplyGapWriteOnlySource, BuilderSource, CargoMetadataSource, MissingRustdocJsonSource,
    NoCachedIrSource, NoExceptionsSource, NotFoundSource, OpenFindingsSource,
    UnknownEtiquetteSource, UnreachableSource,
};
pub use parse::TokenStreamParseError;
use parse::{ConfigSource, JsonParseSource, JsonSource, SynParseSource};
use scan::ScanError;
use store_lock::StoreLockError;
use targets::TargetsError;
use tier::TierError;
use time::ClockSource;
use worklist_cli::WorklistCliError;
use worklist_state::WorklistStateError;

use tracing::instrument;

/// Public error for the `cordial` library (and CLI dispatch).
#[derive(Debug)]
pub struct CordialError {
    kind: Box<CordialErrorKind>,
}

/// Umbrella kind boxed by [`CordialError`].
#[derive(Debug)]
pub enum CordialErrorKind {
    /// A requested clean was refused or incomplete.
    Clean(CleanSource),
    /// Filesystem I/O failed.
    Io(IoSource),
    /// JSON serialization or deserialization failed.
    Json(JsonSource),
    /// A JSON file failed to parse.
    JsonParse(JsonParseSource),
    /// A TOML file failed to parse.
    #[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
    TomlParse(toml_parse::TomlParseSource),
    /// The system clock was before the Unix epoch.
    Clock(ClockSource),
    /// `cordial.toml` (or layered config) failed to load.
    Config(ConfigSource),
    /// `syn` failed to parse a Rust file.
    SynParse(SynParseSource),
    /// A match arm (or equivalent control path) that must not execute.
    Unreachable(UnreachableSource),
    /// `apply_gap` was called for `TracingApplyPolicy::Skip`.
    ApplyGapWriteOnly(ApplyGapWriteOnlySource),
    /// A [`derive_builder`] `build()` left a required field unset.
    Builder(BuilderSource),
    /// No etiquette is registered under that id.
    UnknownEtiquette(UnknownEtiquetteSource),
    /// `cargo metadata` failed.
    CargoMetadata(CargoMetadataSource),
    /// Writing formatted text failed.
    Fmt(FmtSource),
    /// A token stream failed to parse.
    TokenStreamParse(TokenStreamParseError),
    /// A required path was missing.
    NotFound(NotFoundSource),
    /// No exception file exists for that etiquette and crate.
    NoExceptions(NoExceptionsSource),
    /// Cached IR was required but missing.
    NoCachedIr(NoCachedIrSource),
    /// Rustdoc JSON was required but missing.
    MissingRustdocJson(MissingRustdocJsonSource),
    /// Open action items were denied by the CLI.
    OpenFindings(OpenFindingsSource),
    /// A path was not a prefix of another.
    Prefix(PrefixSource),
    /// Built-in or plugin lint-order constraints contain a cycle.
    LintOrderCycle(LintOrderCycleSource),
    /// An `After` edge names an id that is not in the known set.
    LintOrderDangling(LintOrderDanglingSource),
    /// A constraint's own id is not in the known set.
    LintOrderUnknownConstraint(LintOrderUnknownConstraintSource),
    /// A `cargo rustdoc`/`cargo doc` build (or the `amenable dump-registry`
    /// subprocess that shares its shape) failed.
    CargoRustdocBuild(CargoRustdocBuildError),
    /// Resolving or invoking the nightly toolchain for std-family sysroot rustdoc JSON failed.
    CargoRustdocToolchain(CargoRustdocToolchainError),
    /// A `cargo metadata` workspace/dependency lookup failed.
    CargoRustdocMetadata(CargoRustdocMetadataError),
    /// An advisory store-lock operation failed.
    StoreLock(StoreLockError),
    /// An IR scan-engine graph/workspace/loader invariant was violated.
    Graph(GraphError),
    /// A workspace-member/package lookup failed.
    Targets(TargetsError),
    /// A scan-engine probe/rustdoc-cache/reporter/freshness invariant was violated.
    Scan(ScanError),
    /// The worklist CLI's own dispatch failed.
    WorklistCli(WorklistCliError),
    /// `cordial exceptions add`'s own CLI validation failed.
    ExceptionsCli(ExceptionsCliError),
    /// `cordial exceptions edit`'s own validation failed.
    ExceptionsEdit(ExceptionsEditError),
    /// A `--tier` selection failed.
    Tier(TierError),
    /// A worklist item/state mutation failed.
    WorklistState(WorklistStateError),
}

/// Result alias that uses [`CordialError`].
pub type CordialResult<T> = Result<T, CordialError>;

impl CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn from_kind(kind: CordialErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }

    /// Borrowed error kind.
    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> &CordialErrorKind {
        &self.kind
    }
}

impl Display for CordialError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        self.kind.fmt(formatter)
    }
}

impl std::error::Error for CordialError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            CordialErrorKind::Clean(source) => Some(source),
            CordialErrorKind::Io(source) => Some(source),
            CordialErrorKind::Json(source) => Some(source),
            CordialErrorKind::JsonParse(source) => Some(source),
            #[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
            CordialErrorKind::TomlParse(source) => Some(source),
            CordialErrorKind::Clock(source) => Some(source),
            CordialErrorKind::Config(source) => Some(source),
            CordialErrorKind::SynParse(source) => Some(source),
            CordialErrorKind::CargoMetadata(source) => Some(source),
            CordialErrorKind::Fmt(source) => Some(source),
            CordialErrorKind::TokenStreamParse(source) => Some(source),
            CordialErrorKind::Prefix(source) => Some(source),
            CordialErrorKind::Builder(source) => Some(source),
            CordialErrorKind::CargoRustdocBuild(source) => Some(source),
            CordialErrorKind::CargoRustdocToolchain(source) => Some(source),
            CordialErrorKind::CargoRustdocMetadata(source) => Some(source),
            CordialErrorKind::Graph(source) => Some(source),
            CordialErrorKind::Scan(source) => Some(source),
            CordialErrorKind::StoreLock(source) => Some(source),
            CordialErrorKind::Targets(source) => Some(source),
            CordialErrorKind::WorklistCli(source) => Some(source),
            CordialErrorKind::ExceptionsCli(source) => Some(source),
            CordialErrorKind::ExceptionsEdit(source) => Some(source),
            CordialErrorKind::Tier(source) => Some(source),
            CordialErrorKind::WorklistState(source) => Some(source),
            CordialErrorKind::Unreachable(_)
            | CordialErrorKind::ApplyGapWriteOnly(_)
            | CordialErrorKind::UnknownEtiquette(_)
            | CordialErrorKind::NotFound(_)
            | CordialErrorKind::NoExceptions(_)
            | CordialErrorKind::NoCachedIr(_)
            | CordialErrorKind::MissingRustdocJson(_)
            | CordialErrorKind::OpenFindings(_)
            | CordialErrorKind::LintOrderCycle(_)
            | CordialErrorKind::LintOrderDangling(_)
            | CordialErrorKind::LintOrderUnknownConstraint(_) => None,
        }
    }
}

impl Display for CordialErrorKind {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Clean(source) => source.fmt(formatter),
            Self::Io(source) => source.fmt(formatter),
            Self::Json(source) => source.fmt(formatter),
            Self::JsonParse(source) => source.fmt(formatter),
            #[cfg(any(feature = "antipatterns", feature = "dependency_freshness"))]
            Self::TomlParse(source) => source.fmt(formatter),
            Self::Clock(source) => source.fmt(formatter),
            Self::Config(source) => source.fmt(formatter),
            Self::SynParse(source) => source.fmt(formatter),
            Self::Unreachable(source) => source.fmt(formatter),
            Self::ApplyGapWriteOnly(source) => source.fmt(formatter),
            Self::Builder(source) => source.fmt(formatter),
            Self::UnknownEtiquette(source) => source.fmt(formatter),
            Self::CargoMetadata(source) => source.fmt(formatter),
            Self::Fmt(source) => source.fmt(formatter),
            Self::TokenStreamParse(source) => source.fmt(formatter),
            Self::NotFound(source) => source.fmt(formatter),
            Self::NoExceptions(source) => source.fmt(formatter),
            Self::NoCachedIr(source) => source.fmt(formatter),
            Self::MissingRustdocJson(source) => source.fmt(formatter),
            Self::OpenFindings(source) => source.fmt(formatter),
            Self::Prefix(source) => source.fmt(formatter),
            Self::LintOrderCycle(source) => source.fmt(formatter),
            Self::LintOrderDangling(source) => source.fmt(formatter),
            Self::LintOrderUnknownConstraint(source) => source.fmt(formatter),
            Self::CargoRustdocBuild(source) => source.fmt(formatter),
            Self::CargoRustdocToolchain(source) => source.fmt(formatter),
            Self::CargoRustdocMetadata(source) => source.fmt(formatter),
            Self::StoreLock(source) => source.fmt(formatter),
            Self::Graph(source) => source.fmt(formatter),
            Self::Targets(source) => source.fmt(formatter),
            Self::Scan(source) => source.fmt(formatter),
            Self::WorklistCli(source) => source.fmt(formatter),
            Self::ExceptionsCli(source) => source.fmt(formatter),
            Self::ExceptionsEdit(source) => source.fmt(formatter),
            Self::Tier(source) => source.fmt(formatter),
            Self::WorklistState(source) => source.fmt(formatter),
        }
    }
}
