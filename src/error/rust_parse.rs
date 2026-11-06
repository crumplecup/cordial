//! Native parser diagnostics retain the source error and source path.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::panic::Location;
use tracing::instrument;

/// A canonical rust-analyzer source input failed syntax validation.
#[derive(Debug, derive_getters::Getters)]
pub struct RustParseSource {
    #[getter(skip)]
    source: ra_ap_syntax::SyntaxError,
    path: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl RustParseSource {
    #[track_caller]
    #[instrument(level = "debug", skip_all)]
    fn new(path: String, source: ra_ap_syntax::SyntaxError) -> Self {
        let caller = Location::caller();
        Self {
            source,
            path,
            file: caller.file().to_owned(),
            line: caller.line(),
        }
    }
}

impl Display for RustParseSource {
    #[instrument(level = "trace", skip_all)]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(
            formatter,
            "Rust parse error in {}: {}",
            self.path, self.source
        )
    }
}

impl std::error::Error for RustParseSource {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl super::CordialError {
    /// Preserve the native syntax error and the input it came from.
    #[track_caller]
    #[instrument(level = "debug", skip_all)]
    pub fn rust_parse(path: impl Into<String>, source: ra_ap_syntax::SyntaxError) -> Self {
        Self::from_kind(super::CordialErrorKind::RustParse(RustParseSource::new(
            path.into(),
            source,
        )))
    }
}
