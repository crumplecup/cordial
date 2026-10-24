//! Native sources for `cordial`'s worklist CLI dispatch
//! (`cli/worklist.rs`).

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct NextCommandUnrecognizedSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NextCommandUnrecognizedSource {
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

impl std::fmt::Display for NextCommandUnrecognizedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Did you mean `cordial done` or `cordial skip`? ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NextCommandUnrecognizedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NotAWorklistCommandSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NotAWorklistCommandSource {
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

impl std::fmt::Display for NotAWorklistCommandSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "not a worklist command ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NotAWorklistCommandSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoCurrentWorklistSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoCurrentWorklistSource {
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

impl std::fmt::Display for NoCurrentWorklistSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "No current worklist ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for NoCurrentWorklistSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct WorklistOutOfDateSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WorklistOutOfDateSource {
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

impl std::fmt::Display for WorklistOutOfDateSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Worklist is out of date ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for WorklistOutOfDateSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct WorklistValidityIncompleteSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WorklistValidityIncompleteSource {
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

impl std::fmt::Display for WorklistValidityIncompleteSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "worklist validity state is incomplete ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for WorklistValidityIncompleteSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct GateDidNotWriteWorklistSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl GateDidNotWriteWorklistSource {
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

impl std::fmt::Display for GateDidNotWriteWorklistSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "gate did not write a worklist ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for GateDidNotWriteWorklistSource {}

/// Umbrella kind for the worklist CLI's own dispatch failures, nested
/// one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum WorklistCliErrorKind {
    NextCommandUnrecognized(NextCommandUnrecognizedSource),
    NotAWorklistCommand(NotAWorklistCommandSource),
    NoCurrentWorklist(NoCurrentWorklistSource),
    WorklistOutOfDate(WorklistOutOfDateSource),
    WorklistValidityIncomplete(WorklistValidityIncompleteSource),
    GateDidNotWriteWorklist(GateDidNotWriteWorklistSource),
}

#[derive(Debug)]
pub struct WorklistCliError {
    kind: Box<WorklistCliErrorKind>,
    file: String,
    line: u32,
}

impl WorklistCliError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: WorklistCliErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WorklistCliError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            WorklistCliErrorKind::NextCommandUnrecognized(source) => source.fmt(formatter),
            WorklistCliErrorKind::NotAWorklistCommand(source) => source.fmt(formatter),
            WorklistCliErrorKind::NoCurrentWorklist(source) => source.fmt(formatter),
            WorklistCliErrorKind::WorklistOutOfDate(source) => source.fmt(formatter),
            WorklistCliErrorKind::WorklistValidityIncomplete(source) => source.fmt(formatter),
            WorklistCliErrorKind::GateDidNotWriteWorklist(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for WorklistCliError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            WorklistCliErrorKind::NextCommandUnrecognized(source) => Some(source),
            WorklistCliErrorKind::NotAWorklistCommand(source) => Some(source),
            WorklistCliErrorKind::NoCurrentWorklist(source) => Some(source),
            WorklistCliErrorKind::WorklistOutOfDate(source) => Some(source),
            WorklistCliErrorKind::WorklistValidityIncomplete(source) => Some(source),
            WorklistCliErrorKind::GateDidNotWriteWorklist(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `cordial next` is not a recognized worklist command.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn next_command_unrecognized() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::NextCommandUnrecognized(NextCommandUnrecognizedSource::new()),
        )))
    }

    /// A CLI subcommand that is not a worklist command reached worklist dispatch.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn not_a_worklist_command() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::NotAWorklistCommand(NotAWorklistCommandSource::new()),
        )))
    }

    /// No worklist exists yet.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn no_current_worklist() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::NoCurrentWorklist(NoCurrentWorklistSource::new()),
        )))
    }

    /// The on-disk worklist is out of date with the current source tree.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn worklist_out_of_date() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::WorklistOutOfDate(WorklistOutOfDateSource::new()),
        )))
    }

    /// A worklist's validity state did not resolve to a known case.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn worklist_validity_incomplete() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::WorklistValidityIncomplete(
                WorklistValidityIncompleteSource::new(),
            ),
        )))
    }

    /// A gate run completed without writing a worklist.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn gate_did_not_write_worklist() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistCli(WorklistCliError::new(
            WorklistCliErrorKind::GateDidNotWriteWorklist(GateDidNotWriteWorklistSource::new()),
        )))
    }
}
