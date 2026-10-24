//! Native sources for `cordial exceptions edit`'s own validation
//! (`exceptions/edit.rs`).

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct EditRequiresNewValueSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl EditRequiresNewValueSource {
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

impl std::fmt::Display for EditRequiresNewValueSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "edit requires at least one new value (--new-file, --new-line, --new-rule-id, --new-context, or --new-reason) ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for EditRequiresNewValueSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct EtiquetteMustNotBeEmptySource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl EtiquetteMustNotBeEmptySource {
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

impl std::fmt::Display for EtiquetteMustNotBeEmptySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "etiquette must not be empty ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for EtiquetteMustNotBeEmptySource {}

#[derive(Debug, derive_getters::Getters)]
pub struct CrateNameMustNotBeEmptySource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl CrateNameMustNotBeEmptySource {
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

impl std::fmt::Display for CrateNameMustNotBeEmptySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "crate_name must not be empty ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for CrateNameMustNotBeEmptySource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SelectorRequiresAtLeastOneFieldSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SelectorRequiresAtLeastOneFieldSource {
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

impl std::fmt::Display for SelectorRequiresAtLeastOneFieldSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "selector requires at least one of --rule-id, --context, --file, --line ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SelectorRequiresAtLeastOneFieldSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct EditedExceptionDuplicatesRowSource {
    #[getter(skip)]
    rendered: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl EditedExceptionDuplicatesRowSource {
    #[track_caller]
    #[instrument(level = "debug", skip(rendered))]
    pub fn new(rendered: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            rendered: rendered.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for EditedExceptionDuplicatesRowSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "edited exception duplicates an existing row: {} ({}:{})",
            self.rendered, self.file, self.line
        )
    }
}

impl std::error::Error for EditedExceptionDuplicatesRowSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoExceptionMatchesSelectorSource {
    #[getter(skip)]
    etiquette_id: String,
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    selector: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoExceptionMatchesSelectorSource {
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette_id, crate_name, selector))]
    pub fn new(
        etiquette_id: impl Into<String>,
        crate_name: impl Into<String>,
        selector: impl Into<String>,
    ) -> Self {
        let loc = Location::caller();
        Self {
            etiquette_id: etiquette_id.into(),
            crate_name: crate_name.into(),
            selector: selector.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for NoExceptionMatchesSelectorSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "no exception for etiquette `{}` crate `{}` matches {} ({}:{})",
            self.etiquette_id, self.crate_name, self.selector, self.file, self.line
        )
    }
}

impl std::error::Error for NoExceptionMatchesSelectorSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct MultipleExceptionsMatchSelectorSource {
    #[getter(copy)]
    count: usize,
    #[getter(skip)]
    etiquette_id: String,
    #[getter(skip)]
    crate_name: String,
    #[getter(skip)]
    selector: String,
    #[getter(skip)]
    listing: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl MultipleExceptionsMatchSelectorSource {
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette_id, crate_name, selector, listing))]
    pub fn new(
        count: usize,
        etiquette_id: impl Into<String>,
        crate_name: impl Into<String>,
        selector: impl Into<String>,
        listing: impl Into<String>,
    ) -> Self {
        let loc = Location::caller();
        Self {
            count,
            etiquette_id: etiquette_id.into(),
            crate_name: crate_name.into(),
            selector: selector.into(),
            listing: listing.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for MultipleExceptionsMatchSelectorSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} exceptions for etiquette `{}` crate `{}` match {}; add more selector fields to pick one:{} ({}:{})",
            self.count,
            self.etiquette_id,
            self.crate_name,
            self.selector,
            self.listing,
            self.file,
            self.line
        )
    }
}

impl std::error::Error for MultipleExceptionsMatchSelectorSource {}

/// Umbrella kind for `cordial exceptions edit`'s own validation
/// failures, nested one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum ExceptionsEditErrorKind {
    EditRequiresNewValue(EditRequiresNewValueSource),
    EtiquetteMustNotBeEmpty(EtiquetteMustNotBeEmptySource),
    CrateNameMustNotBeEmpty(CrateNameMustNotBeEmptySource),
    SelectorRequiresAtLeastOneField(SelectorRequiresAtLeastOneFieldSource),
    EditedExceptionDuplicatesRow(EditedExceptionDuplicatesRowSource),
    NoExceptionMatchesSelector(NoExceptionMatchesSelectorSource),
    MultipleExceptionsMatchSelector(MultipleExceptionsMatchSelectorSource),
}

#[derive(Debug)]
pub struct ExceptionsEditError {
    kind: Box<ExceptionsEditErrorKind>,
    file: String,
    line: u32,
}

impl ExceptionsEditError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: ExceptionsEditErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for ExceptionsEditError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            ExceptionsEditErrorKind::EditRequiresNewValue(source) => source.fmt(formatter),
            ExceptionsEditErrorKind::EtiquetteMustNotBeEmpty(source) => source.fmt(formatter),
            ExceptionsEditErrorKind::CrateNameMustNotBeEmpty(source) => source.fmt(formatter),
            ExceptionsEditErrorKind::SelectorRequiresAtLeastOneField(source) => {
                source.fmt(formatter)
            }
            ExceptionsEditErrorKind::EditedExceptionDuplicatesRow(source) => source.fmt(formatter),
            ExceptionsEditErrorKind::NoExceptionMatchesSelector(source) => source.fmt(formatter),
            ExceptionsEditErrorKind::MultipleExceptionsMatchSelector(source) => {
                source.fmt(formatter)
            }
        }
    }
}

impl std::error::Error for ExceptionsEditError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            ExceptionsEditErrorKind::EditRequiresNewValue(source) => Some(source),
            ExceptionsEditErrorKind::EtiquetteMustNotBeEmpty(source) => Some(source),
            ExceptionsEditErrorKind::CrateNameMustNotBeEmpty(source) => Some(source),
            ExceptionsEditErrorKind::SelectorRequiresAtLeastOneField(source) => Some(source),
            ExceptionsEditErrorKind::EditedExceptionDuplicatesRow(source) => Some(source),
            ExceptionsEditErrorKind::NoExceptionMatchesSelector(source) => Some(source),
            ExceptionsEditErrorKind::MultipleExceptionsMatchSelector(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `cordial exceptions edit` was used with no new value to set.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn edit_requires_new_value() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::EditRequiresNewValue(
                EditRequiresNewValueSource::new(),
            )),
        ))
    }

    /// An edited exception row would duplicate an existing one.
    #[track_caller]
    #[instrument(level = "debug", skip(rendered))]
    pub fn edited_exception_duplicates_row(rendered: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::EditedExceptionDuplicatesRow(
                EditedExceptionDuplicatesRowSource::new(rendered),
            )),
        ))
    }

    /// `cordial exceptions edit`'s `etiquette_id` argument was empty.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn etiquette_must_not_be_empty() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::EtiquetteMustNotBeEmpty(
                EtiquetteMustNotBeEmptySource::new(),
            )),
        ))
    }

    /// `cordial exceptions edit`'s `crate_name` argument was empty.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn crate_name_must_not_be_empty() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::CrateNameMustNotBeEmpty(
                CrateNameMustNotBeEmptySource::new(),
            )),
        ))
    }

    /// An exception selector named none of `--rule-id`/`--context`/`--file`/`--line`.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn selector_requires_at_least_one_field() -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::SelectorRequiresAtLeastOneField(
                SelectorRequiresAtLeastOneFieldSource::new(),
            )),
        ))
    }

    /// No exception matched an edit selector.
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette_id, crate_name, selector))]
    pub fn no_exception_matches_selector(
        etiquette_id: impl Into<String>,
        crate_name: impl Into<String>,
        selector: impl Into<String>,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::NoExceptionMatchesSelector(
                NoExceptionMatchesSelectorSource::new(etiquette_id, crate_name, selector),
            )),
        ))
    }

    /// Several exceptions matched an edit selector that must pick exactly one.
    #[track_caller]
    #[instrument(level = "debug", skip(etiquette_id, crate_name, selector, listing))]
    pub fn multiple_exceptions_match_selector(
        count: usize,
        etiquette_id: impl Into<String>,
        crate_name: impl Into<String>,
        selector: impl Into<String>,
        listing: impl Into<String>,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::ExceptionsEdit(
            ExceptionsEditError::new(ExceptionsEditErrorKind::MultipleExceptionsMatchSelector(
                MultipleExceptionsMatchSelectorSource::new(
                    count,
                    etiquette_id,
                    crate_name,
                    selector,
                    listing,
                ),
            )),
        ))
    }
}
