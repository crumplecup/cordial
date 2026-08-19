//! Crate error: parent boxes a Kind; Kind variants are native sources.

mod io;
mod local;
mod parse;

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::{PathBuf, StripPrefixError};

use io::{FmtSource, IoSource, PrefixSource};
use local::{
    CargoMetadataSource, InvariantSource, NoCachedIrSource, NoExceptionsSource, NotFoundSource,
    UnknownEtiquetteSource,
};
pub use parse::TokenStreamParseError;
use parse::{ConfigSource, JsonParseSource, JsonSource, SynParseSource};

use tracing::instrument;

/// Public error for the `cordial` library (and CLI dispatch).
#[derive(Debug)]
pub struct CordialError {
    kind: Box<CordialErrorKind>,
}

/// Umbrella kind boxed by [`CordialError`].
#[derive(Debug)]
pub enum CordialErrorKind {
    Io(IoSource),
    Json(JsonSource),
    JsonParse(JsonParseSource),
    Config(ConfigSource),
    SynParse(SynParseSource),
    Invariant(InvariantSource),
    UnknownEtiquette(UnknownEtiquetteSource),
    CargoMetadata(CargoMetadataSource),
    Fmt(FmtSource),
    TokenStreamParse(TokenStreamParseError),
    NotFound(NotFoundSource),
    NoExceptions(NoExceptionsSource),
    NoCachedIr(NoCachedIrSource),
    Prefix(PrefixSource),
}

pub type CordialResult<T> = Result<T, CordialError>;

impl CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind), ret)]
    fn from_kind(kind: CordialErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn kind(&self) -> &CordialErrorKind {
        &self.kind
    }

    #[track_caller]
    #[instrument(level = "debug", skip(message))]
    pub fn invariant(message: impl Into<String>) -> Self {
        Self::from_kind(CordialErrorKind::Invariant(InvariantSource::new(message)))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(id))]
    pub fn unknown_etiquette(id: impl Into<String>) -> Self {
        Self::from_kind(CordialErrorKind::UnknownEtiquette(
            UnknownEtiquetteSource::new(id),
        ))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(path, err))]
    pub fn syn_parse(path: impl Into<String>, err: syn::Error) -> Self {
        Self::from_kind(CordialErrorKind::SynParse(SynParseSource::new(path, err)))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(path, err))]
    pub fn json_parse(path: impl Into<String>, err: serde_json::Error) -> Self {
        Self::from_kind(CordialErrorKind::JsonParse(JsonParseSource::new(path, err)))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(err))]
    pub fn cargo_metadata(err: cargo_metadata::Error) -> Self {
        Self::from(err)
    }

    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn not_found(path: PathBuf) -> Self {
        Self::from_kind(CordialErrorKind::NotFound(NotFoundSource::new(path)))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(etiquette, crate_name))]
    pub fn no_exceptions(etiquette: impl Into<String>, crate_name: impl Into<String>) -> Self {
        Self::from_kind(CordialErrorKind::NoExceptions(NoExceptionsSource::new(
            etiquette, crate_name,
        )))
    }

    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn no_cached_ir(path: PathBuf) -> Self {
        Self::from_kind(CordialErrorKind::NoCachedIr(NoCachedIrSource::new(path)))
    }
}

impl Display for CordialError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        self.kind.fmt(formatter)
    }
}

impl std::error::Error for CordialError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            CordialErrorKind::Io(source) => Some(source),
            CordialErrorKind::Json(source) => Some(source),
            CordialErrorKind::JsonParse(source) => Some(source),
            CordialErrorKind::Config(source) => Some(source),
            CordialErrorKind::SynParse(source) => Some(source),
            CordialErrorKind::CargoMetadata(source) => Some(source),
            CordialErrorKind::Fmt(source) => Some(source),
            CordialErrorKind::TokenStreamParse(source) => Some(source),
            CordialErrorKind::Prefix(source) => Some(source),
            CordialErrorKind::Invariant(_)
            | CordialErrorKind::UnknownEtiquette(_)
            | CordialErrorKind::NotFound(_)
            | CordialErrorKind::NoExceptions(_)
            | CordialErrorKind::NoCachedIr(_) => None,
        }
    }
}

impl Display for CordialErrorKind {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Io(source) => source.fmt(formatter),
            Self::Json(source) => source.fmt(formatter),
            Self::JsonParse(source) => source.fmt(formatter),
            Self::Config(source) => source.fmt(formatter),
            Self::SynParse(source) => source.fmt(formatter),
            Self::Invariant(source) => source.fmt(formatter),
            Self::UnknownEtiquette(source) => source.fmt(formatter),
            Self::CargoMetadata(source) => source.fmt(formatter),
            Self::Fmt(source) => source.fmt(formatter),
            Self::TokenStreamParse(source) => source.fmt(formatter),
            Self::NotFound(source) => source.fmt(formatter),
            Self::NoExceptions(source) => source.fmt(formatter),
            Self::NoCachedIr(source) => source.fmt(formatter),
            Self::Prefix(source) => source.fmt(formatter),
        }
    }
}

impl From<std::io::Error> for CordialError {
    #[track_caller]
    fn from(value: std::io::Error) -> Self {
        Self::from_kind(CordialErrorKind::Io(IoSource::from(value)))
    }
}

impl From<serde_json::Error> for CordialError {
    #[track_caller]
    fn from(value: serde_json::Error) -> Self {
        Self::from_kind(CordialErrorKind::Json(JsonSource::from(value)))
    }
}

impl From<config::ConfigError> for CordialError {
    #[track_caller]
    fn from(value: config::ConfigError) -> Self {
        Self::from_kind(CordialErrorKind::Config(ConfigSource::from(value)))
    }
}

impl From<cargo_metadata::Error> for CordialError {
    #[track_caller]
    fn from(value: cargo_metadata::Error) -> Self {
        Self::from_kind(CordialErrorKind::CargoMetadata(CargoMetadataSource::from(
            value,
        )))
    }
}

impl From<std::fmt::Error> for CordialError {
    #[track_caller]
    fn from(value: std::fmt::Error) -> Self {
        Self::from_kind(CordialErrorKind::Fmt(FmtSource::from(value)))
    }
}

impl From<proc_macro2::LexError> for CordialError {
    #[track_caller]
    fn from(value: proc_macro2::LexError) -> Self {
        Self::from_kind(CordialErrorKind::TokenStreamParse(
            TokenStreamParseError::from(value),
        ))
    }
}

impl From<StripPrefixError> for CordialError {
    #[track_caller]
    #[instrument(level = "debug", skip(value), ret)]
    fn from(value: StripPrefixError) -> Self {
        Self::from_kind(CordialErrorKind::Prefix(PrefixSource::from(value)))
    }
}
