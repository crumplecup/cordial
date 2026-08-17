use std::io;
use std::path::{PathBuf, StripPrefixError};

use cordial::CordialError;
use derive_more::Display;

pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug, Display)]
pub enum CliError {
    #[display("I/O error: {_0}")]
    Io(io::Error),
    #[display("{_0}")]
    Cordial(CordialError),
    #[display("not found: {}", path.display())]
    NotFound { path: PathBuf },
    #[display("no exceptions for etiquette `{etiquette}` crate `{crate_name}`")]
    NoExceptions {
        etiquette: String,
        crate_name: String,
    },
    #[display("no cached IR at {} — run `cordial run` first", path.display())]
    NoCachedIr { path: PathBuf },
    #[display(
        "coverage commands require a coverage feature; rebuild with --features elicitation or homecoming_std"
    )]
    CoverageFeatureDisabled,
    #[display("path is not under the store prefix: {_0}")]
    Prefix(StripPrefixError),
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Cordial(err) => Some(err),
            Self::Prefix(err) => Some(err),
            Self::NotFound { .. }
            | Self::NoExceptions { .. }
            | Self::NoCachedIr { .. }
            | Self::CoverageFeatureDisabled => None,
        }
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<CordialError> for CliError {
    fn from(value: CordialError) -> Self {
        Self::Cordial(value)
    }
}

impl From<StripPrefixError> for CliError {
    fn from(value: StripPrefixError) -> Self {
        Self::Prefix(value)
    }
}
