//! Error-handling plugin semantics — shared supertrait for the error analysis family.

use std::path::Path;

use crate::error::CordialResult;
use crate::loader::CrateTarget;
use crate::plugin::{Plugin, PluginCategory};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use tracing::instrument;

/// Profile policy: which layers run and how findings are classified.
pub trait ErrorHandlingPolicy: Send + Sync {
    /// Layers.
    fn layers(&self) -> ErrorHandlingLayers;
}

/// Discovers crate scopes for an error-handling run (default: workspace members).
pub trait ErrorScopeProvider: Send + Sync {
    /// Error scopes.
    fn error_scopes(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<ErrorScope>>;
}

/// Semantic supertrait: error flow analysis over workspace source IR.
pub trait ErrorHandling: Plugin {
    /// Scope provider.
    fn scope_provider(&self) -> &dyn ErrorScopeProvider;
    /// Policy.
    fn policy(&self) -> &dyn ErrorHandlingPolicy;

    /// Scopes.
    fn scopes(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<ErrorScope>> {
        self.scope_provider().error_scopes(session, filter)
    }

    /// Etiquette / lint category this rule belongs to.
    fn category(&self) -> PluginCategory {
        PluginCategory::ErrorHandling
    }
}

/// One crate in scope for error-flow analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorScope {
    /// Cargo package name.
    pub crate_name: String,
}

impl ErrorScope {
    /// Error-analysis scope for a workspace member crate.
    #[instrument(level = "debug", skip(crate_name))]
    pub fn workspace_member(crate_name: impl Into<String>) -> Self {
        Self {
            crate_name: crate_name.into(),
        }
    }
}

/// Which error-analysis layers a profile enables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorHandlingLayers {
    /// Abort sites (`panic!`, `unwrap`, `expect`, `unreachable!`).
    pub panics: bool,
    /// Error-site scanning (`?`, `map_err`, …).
    pub sites: bool,
    /// Error-chain / `source()` preservation scanning.
    pub chain: bool,
    /// Library code should return a crate error type, not panic.
    pub internal: bool,
    /// Keep foreign error types in the `source()` chain.
    pub foreign_types: bool,
    /// Do not stringify or discard typed errors.
    pub attenuation: bool,
}

impl ErrorHandlingLayers {
    /// Fully enabled error-handling policy.
    pub const FULL: Self = Self {
        panics: true,
        sites: true,
        chain: true,
        internal: true,
        foreign_types: true,
        attenuation: true,
    };

    /// Any enabled.
    #[instrument(level = "debug", skip(self))]
    pub fn any_enabled(self) -> bool {
        self.panics
            || self.sites
            || self.chain
            || self.internal
            || self.foreign_types
            || self.attenuation
    }
}

/// Where a failure site lives — replacement stack differs by surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSurface {
    /// `src/**` library code (not `main.rs` / `src/bin`).
    Library,
    /// Binary entrypoints (`src/main.rs`, `src/bin/**`, `examples/**`).
    Binary,
    /// Test and bench code (`tests/**`, `benches/**`, `src/tests/**`).
    Test,
}

impl ErrorSurface {
    /// Classify a source path as library, binary, or test.
    #[instrument(level = "debug", skip(path), ret)]
    pub fn from_path(path: &Path) -> Self {
        let components: Vec<&str> = path
            .iter()
            .filter_map(|component| component.to_str())
            .collect();
        // Classify from Cargo layout (`src/`, package-level `tests/` / `examples/`),
        // not from incidental directory names earlier in the absolute path.
        if let Some(src_idx) = components.iter().rposition(|component| *component == "src") {
            return match components.get(src_idx + 1).copied() {
                Some("main.rs") | Some("bin") => Self::Binary,
                Some("tests") => Self::Test,
                _ => Self::Library,
            };
        }
        if components.contains(&"tests") || components.contains(&"benches") {
            return Self::Test;
        }
        if components.contains(&"examples") {
            return Self::Binary;
        }
        Self::Library
    }

    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Binary => "binary",
            Self::Test => "test",
        }
    }

    /// Expected error stack for this surface.
    #[instrument(level = "debug", skip(self))]
    pub fn expected_stack(self) -> &'static str {
        match self {
            Self::Library => "internal error types",
            Self::Binary | Self::Test => "miette",
        }
    }

    /// Checklist action for an abort site on this surface.
    #[instrument(level = "debug", skip(self))]
    pub fn abort_action(self) -> &'static str {
        match self {
            Self::Library => "return the crate's internal error type instead of aborting",
            Self::Binary | Self::Test => "surface this with miette instead of aborting",
        }
    }
}

impl std::fmt::Display for ErrorSurface {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Default provider: one scope per workspace member from `cargo metadata`.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorkspaceMembersErrorScopeProvider;

impl ErrorScopeProvider for WorkspaceMembersErrorScopeProvider {
    #[instrument(level = "trace", skip(self, session, filter))]
    fn error_scopes(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<ErrorScope>> {
        let members = discover_crate_targets(session.project_root(), filter)?;
        Ok(members
            .into_iter()
            .map(|target: CrateTarget| ErrorScope::workspace_member(target.crate_name))
            .collect())
    }
}

/// Standard workspace policy: panics plus the Result/chain stack.
#[derive(Debug, Default, Clone, Copy)]
pub struct StandardErrorHandlingPolicy;

impl ErrorHandlingPolicy for StandardErrorHandlingPolicy {
    #[instrument(level = "trace", skip(self))]
    fn layers(&self) -> ErrorHandlingLayers {
        ErrorHandlingLayers::FULL
    }
}
