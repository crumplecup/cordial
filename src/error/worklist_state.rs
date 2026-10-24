//! Native sources for worklist item/state mutation
//! (`worklist/item.rs`, `worklist/state.rs`).

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct DistinctScoringKeysShareDigestSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl DistinctScoringKeysShareDigestSource {
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

impl std::fmt::Display for DistinctScoringKeysShareDigestSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "distinct scoring keys share a full digest ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for DistinctScoringKeysShareDigestSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct MissingWorklistIdPrefixSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl MissingWorklistIdPrefixSource {
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

impl std::fmt::Display for MissingWorklistIdPrefixSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "missing worklist id prefix ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for MissingWorklistIdPrefixSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct AreaOpenItemCountMismatchSource {
    #[getter(skip)]
    area_title: String,
    #[getter(copy)]
    report_open: usize,
    #[getter(copy)]
    checklist_count: usize,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AreaOpenItemCountMismatchSource {
    #[track_caller]
    #[instrument(level = "debug", skip(area_title))]
    pub fn new(area_title: impl Into<String>, report_open: usize, checklist_count: usize) -> Self {
        let loc = Location::caller();
        Self {
            area_title: area_title.into(),
            report_open,
            checklist_count,
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for AreaOpenItemCountMismatchSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: report has {} open items but checklists have {} ({}:{})",
            self.area_title, self.report_open, self.checklist_count, self.file, self.line
        )
    }
}

impl std::error::Error for AreaOpenItemCountMismatchSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct StarMustBeAloneSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl StarMustBeAloneSource {
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

impl std::fmt::Display for StarMustBeAloneSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Use `cordial done '*'` by itself, or name item ids without `*` ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for StarMustBeAloneSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoCurrentItemSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoCurrentItemSource {
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

impl std::fmt::Display for NoCurrentItemSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "No current item ({}:{})", self.file, self.line)
    }
}

impl std::error::Error for NoCurrentItemSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct NoItemOnListSource {
    #[getter(skip)]
    id: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl NoItemOnListSource {
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

impl std::fmt::Display for NoItemOnListSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "No item {} on this list ({}:{})",
            self.id, self.file, self.line
        )
    }
}

impl std::error::Error for NoItemOnListSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct ItemAlreadyDoneCannotSkipSource {
    #[getter(skip)]
    id: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl ItemAlreadyDoneCannotSkipSource {
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

impl std::fmt::Display for ItemAlreadyDoneCannotSkipSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Item {} is already checked off and cannot be skipped ({}:{})",
            self.id, self.file, self.line
        )
    }
}

impl std::error::Error for ItemAlreadyDoneCannotSkipSource {}

/// Umbrella kind for worklist item/state mutation failures, nested
/// one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum WorklistStateErrorKind {
    DistinctScoringKeysShareDigest(DistinctScoringKeysShareDigestSource),
    MissingWorklistIdPrefix(MissingWorklistIdPrefixSource),
    AreaOpenItemCountMismatch(AreaOpenItemCountMismatchSource),
    StarMustBeAlone(StarMustBeAloneSource),
    NoCurrentItem(NoCurrentItemSource),
    NoItemOnList(NoItemOnListSource),
    ItemAlreadyDoneCannotSkip(ItemAlreadyDoneCannotSkipSource),
}

#[derive(Debug)]
pub struct WorklistStateError {
    kind: Box<WorklistStateErrorKind>,
    file: String,
    line: u32,
}

impl WorklistStateError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: WorklistStateErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WorklistStateError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            WorklistStateErrorKind::DistinctScoringKeysShareDigest(source) => source.fmt(formatter),
            WorklistStateErrorKind::MissingWorklistIdPrefix(source) => source.fmt(formatter),
            WorklistStateErrorKind::AreaOpenItemCountMismatch(source) => source.fmt(formatter),
            WorklistStateErrorKind::StarMustBeAlone(source) => source.fmt(formatter),
            WorklistStateErrorKind::NoCurrentItem(source) => source.fmt(formatter),
            WorklistStateErrorKind::NoItemOnList(source) => source.fmt(formatter),
            WorklistStateErrorKind::ItemAlreadyDoneCannotSkip(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for WorklistStateError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            WorklistStateErrorKind::DistinctScoringKeysShareDigest(source) => Some(source),
            WorklistStateErrorKind::MissingWorklistIdPrefix(source) => Some(source),
            WorklistStateErrorKind::AreaOpenItemCountMismatch(source) => Some(source),
            WorklistStateErrorKind::StarMustBeAlone(source) => Some(source),
            WorklistStateErrorKind::NoCurrentItem(source) => Some(source),
            WorklistStateErrorKind::NoItemOnList(source) => Some(source),
            WorklistStateErrorKind::ItemAlreadyDoneCannotSkip(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// Two distinct worklist scoring keys hashed to the same full digest.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn distinct_scoring_keys_share_digest() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::DistinctScoringKeysShareDigest(
                DistinctScoringKeysShareDigestSource::new(),
            )),
        ))
    }

    /// A worklist item's id prefix was not computed before use.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn missing_worklist_id_prefix() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::MissingWorklistIdPrefix(
                MissingWorklistIdPrefixSource::new(),
            )),
        ))
    }

    /// A quality report area's open-item count did not match its checklist.
    #[track_caller]
    #[instrument(level = "debug", skip(area_title))]
    pub fn area_open_item_count_mismatch(
        area_title: impl Into<String>,
        report_open: usize,
        checklist_count: usize,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::AreaOpenItemCountMismatch(
                AreaOpenItemCountMismatchSource::new(area_title, report_open, checklist_count),
            )),
        ))
    }

    /// `cordial done '*'` was combined with other item ids.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn star_must_be_alone() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::StarMustBeAlone(
                StarMustBeAloneSource::new(),
            )),
        ))
    }

    /// `cordial done`/`skip` ran with no ids and no current item.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn no_current_item() -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::NoCurrentItem(
                NoCurrentItemSource::new(),
            )),
        ))
    }

    /// A named worklist item id was not found on the current list.
    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn no_item_on_list(id: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::NoItemOnList(
                NoItemOnListSource::new(id),
            )),
        ))
    }

    /// `cordial skip` was used on an item already marked done.
    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn item_already_done_cannot_skip(id: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::WorklistState(
            WorklistStateError::new(WorklistStateErrorKind::ItemAlreadyDoneCannotSkip(
                ItemAlreadyDoneCannotSkipSource::new(id),
            )),
        ))
    }
}
