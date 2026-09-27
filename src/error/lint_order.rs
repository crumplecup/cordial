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
    #[instrument(level = "debug", ret)]
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
    #[instrument(level = "debug", skip(from, missing), ret)]
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
    #[instrument(level = "debug", skip(id), ret)]
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
