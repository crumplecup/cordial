//! Native sources for lint-order table validation.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct LintOrderCycleSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl LintOrderCycleSource {
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

impl Display for LintOrderCycleSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "lint order constraints contain a cycle")
    }
}

impl std::error::Error for LintOrderCycleSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct LintOrderDanglingSource {
    #[getter(skip)]
    from: String,
    #[getter(skip)]
    missing: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl LintOrderDanglingSource {
    #[track_caller]
    #[instrument(level = "debug", skip(from, missing))]
    pub fn new(from: impl Into<String>, missing: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            from: from.into(),
            missing: missing.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for LintOrderDanglingSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "lint `{}` after unknown id `{}`",
            self.from, self.missing
        )
    }
}

impl std::error::Error for LintOrderDanglingSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct LintOrderUnknownConstraintSource {
    #[getter(skip)]
    id: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl LintOrderUnknownConstraintSource {
    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn new(id: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            id: id.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl Display for LintOrderUnknownConstraintSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "lint order constraint for unknown id `{}`",
            self.id
        )
    }
}

impl std::error::Error for LintOrderUnknownConstraintSource {}

impl super::CordialError {
    /// Lint-order `After` edges contain a cycle.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn lint_order_cycle() -> Self {
        Self::from_kind(super::CordialErrorKind::LintOrderCycle(
            LintOrderCycleSource::new(),
        ))
    }

    /// `from` names `missing`, which is not in the known lint-id set.
    #[track_caller]
    #[instrument(level = "debug", skip(from, missing))]
    pub fn lint_order_dangling(from: impl Into<String>, missing: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::LintOrderDangling(
            LintOrderDanglingSource::new(from, missing),
        ))
    }

    /// Constraint `id` is not in the known lint-id set.
    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn lint_order_unknown_constraint(id: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::LintOrderUnknownConstraint(
            LintOrderUnknownConstraintSource::new(id),
        ))
    }
}
