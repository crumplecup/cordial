//! I/O, fmt, and path-prefix native sources.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;

use tracing::instrument;

#[derive(Debug)]
pub struct IoSource {
    source: std::io::Error,
    location: &'static Location<'static>,
}

impl IoSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: std::io::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl From<std::io::Error> for IoSource {
    #[track_caller]
    fn from(source: std::io::Error) -> Self {
        Self::new(source)
    }
}

impl Display for IoSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "I/O error: {}", self.source)
    }
}

impl std::error::Error for IoSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub struct FmtSource {
    source: std::fmt::Error,
    location: &'static Location<'static>,
}

impl FmtSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: std::fmt::Error) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl From<std::fmt::Error> for FmtSource {
    #[track_caller]
    fn from(source: std::fmt::Error) -> Self {
        Self::new(source)
    }
}

impl Display for FmtSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "fmt error: {}", self.source)
    }
}

impl std::error::Error for FmtSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub struct PrefixSource {
    source: std::path::StripPrefixError,
    location: &'static Location<'static>,
}

impl PrefixSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    pub fn new(source: std::path::StripPrefixError) -> Self {
        Self {
            source,
            location: Location::caller(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl From<std::path::StripPrefixError> for PrefixSource {
    #[track_caller]
    #[instrument(level = "debug", skip(source), ret)]
    fn from(source: std::path::StripPrefixError) -> Self {
        Self::new(source)
    }
}

impl Display for PrefixSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "path is not under the store prefix: {}",
            self.source
        )
    }
}

impl std::error::Error for PrefixSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
