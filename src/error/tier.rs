//! Native sources for `cordial`'s own `--tier` selection (`tier/select.rs`).

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct AllMustBeOnlyTierRequestedSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl AllMustBeOnlyTierRequestedSource {
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

impl std::fmt::Display for AllMustBeOnlyTierRequestedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "`all` must be the only tier requested ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for AllMustBeOnlyTierRequestedSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct UnknownTierSource {
    #[getter(skip)]
    tier: String,
    #[getter(skip)]
    known: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl UnknownTierSource {
    #[track_caller]
    #[instrument(level = "debug", skip(tier, known))]
    pub fn new(tier: impl Into<String>, known: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            tier: tier.into(),
            known: known.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for UnknownTierSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Unknown tier `{}`; known tiers: {} ({}:{})",
            self.tier, self.known, self.file, self.line
        )
    }
}

impl std::error::Error for UnknownTierSource {}

/// Umbrella kind for `--tier` selection failures, nested one level
/// under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum TierErrorKind {
    AllMustBeOnlyTierRequested(AllMustBeOnlyTierRequestedSource),
    UnknownTier(UnknownTierSource),
}

#[derive(Debug)]
pub struct TierError {
    kind: Box<TierErrorKind>,
    file: String,
    line: u32,
}

impl TierError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: TierErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for TierError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            TierErrorKind::AllMustBeOnlyTierRequested(source) => source.fmt(formatter),
            TierErrorKind::UnknownTier(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for TierError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            TierErrorKind::AllMustBeOnlyTierRequested(source) => Some(source),
            TierErrorKind::UnknownTier(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// `--tier all` was combined with another tier name.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn all_must_be_only_tier_requested() -> Self {
        Self::from_kind(super::CordialErrorKind::Tier(TierError::new(
            TierErrorKind::AllMustBeOnlyTierRequested(AllMustBeOnlyTierRequestedSource::new()),
        )))
    }

    /// An unrecognized tier name was requested.
    #[track_caller]
    #[instrument(level = "debug", skip(tier, known))]
    pub fn unknown_tier(tier: impl Into<String>, known: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Tier(TierError::new(
            TierErrorKind::UnknownTier(UnknownTierSource::new(tier, known)),
        )))
    }
}
