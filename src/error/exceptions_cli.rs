//! Native sources for `cordial exceptions add`'s own CLI validation
//! (`cli/exceptions.rs`, `exceptions/add.rs`, `exceptions/entry.rs`).

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct CoverageSkipRequiresPathSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CoverageSkipRequiresPathSource {
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

impl std::fmt::Display for CoverageSkipRequiresPathSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "coverage skip requires --path ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for CoverageSkipRequiresPathSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct QualityExceptionRequiresEtiquetteSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl QualityExceptionRequiresEtiquetteSource {
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

impl std::fmt::Display for QualityExceptionRequiresEtiquetteSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "quality exception requires an etiquette ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for QualityExceptionRequiresEtiquetteSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct QualityExceptionRequiresFileSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl QualityExceptionRequiresFileSource {
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

impl std::fmt::Display for QualityExceptionRequiresFileSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "quality exception requires --file ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for QualityExceptionRequiresFileSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct LabelMustNotBeEmptySource {
    #[getter(skip)]
    label: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl LabelMustNotBeEmptySource {
    #[track_caller]
    #[instrument(level = "debug", skip(label))]
    pub fn new(label: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            label: label.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for LabelMustNotBeEmptySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} must not be empty ({}:{})",
            self.label, self.file, self.line
        )
    }
}

impl std::error::Error for LabelMustNotBeEmptySource {}

/// Umbrella kind for `cordial exceptions add`'s own CLI validation
/// failures, nested one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum ExceptionsCliErrorKind {
    CoverageSkipRequiresPath(CoverageSkipRequiresPathSource),
    QualityExceptionRequiresEtiquette(QualityExceptionRequiresEtiquetteSource),
    QualityExceptionRequiresFile(QualityExceptionRequiresFileSource),
    LabelMustNotBeEmpty(LabelMustNotBeEmptySource),
}

#[derive(Debug)]
pub struct ExceptionsCliError {
    kind: Box<ExceptionsCliErrorKind>,
    file: String,
    line: u32,
}

impl ExceptionsCliError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: ExceptionsCliErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for ExceptionsCliError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            ExceptionsCliErrorKind::CoverageSkipRequiresPath(source) => source.fmt(formatter),
            ExceptionsCliErrorKind::QualityExceptionRequiresEtiquette(source) => {
                source.fmt(formatter)
            }
            ExceptionsCliErrorKind::QualityExceptionRequiresFile(source) => source.fmt(formatter),
            ExceptionsCliErrorKind::LabelMustNotBeEmpty(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for ExceptionsCliError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            ExceptionsCliErrorKind::CoverageSkipRequiresPath(source) => Some(source),
            ExceptionsCliErrorKind::QualityExceptionRequiresEtiquette(source) => Some(source),
            ExceptionsCliErrorKind::QualityExceptionRequiresFile(source) => Some(source),
            ExceptionsCliErrorKind::LabelMustNotBeEmpty(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `cordial exceptions add --patch-set` was used without `--path`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn coverage_skip_requires_path() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsCli(
            ExceptionsCliError::new(ExceptionsCliErrorKind::CoverageSkipRequiresPath(
                CoverageSkipRequiresPathSource::new(),
            )),
        ))
    }

    /// `cordial exceptions add` was used without `--etiquette`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn quality_exception_requires_etiquette() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsCli(
            ExceptionsCliError::new(ExceptionsCliErrorKind::QualityExceptionRequiresEtiquette(
                QualityExceptionRequiresEtiquetteSource::new(),
            )),
        ))
    }

    /// `cordial exceptions add` was used without `--file`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn quality_exception_requires_file() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsCli(
            ExceptionsCliError::new(ExceptionsCliErrorKind::QualityExceptionRequiresFile(
                QualityExceptionRequiresFileSource::new(),
            )),
        ))
    }

    /// A required label (etiquette, crate name, ...) was empty.
    #[track_caller]
    #[instrument(level = "debug", skip(label))]
    pub fn label_must_not_be_empty(label: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsCli(
            ExceptionsCliError::new(ExceptionsCliErrorKind::LabelMustNotBeEmpty(
                LabelMustNotBeEmptySource::new(label),
            )),
        ))
    }
}
