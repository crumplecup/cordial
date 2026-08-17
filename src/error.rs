use derive_more::Display;

#[derive(Debug, Display)]
pub enum CordialError {
    #[display("I/O error: {_0}")]
    Io(std::io::Error),
    #[display("JSON error: {_0}")]
    Json(serde_json::Error),
    #[display("JSON error in {path}: {err}")]
    JsonParse {
        path: String,
        err: serde_json::Error,
    },
    #[display("config error: {_0}")]
    Config(config::ConfigError),
    #[display("syn parse error in {path}: {err}")]
    SynParse { path: String, err: syn::Error },
    #[display("invariant violated: {message}")]
    Invariant { message: String },
    #[display("etiquette not registered: {id}")]
    UnknownEtiquette { id: String },
    #[display("cargo metadata error: {_0}")]
    CargoMetadata(cargo_metadata::Error),
    #[display("fmt error: {_0}")]
    Fmt(std::fmt::Error),
    #[display("token stream parse error: {_0}")]
    TokenStreamParse(TokenStreamParseError),
}

/// Send/Sync stand-in for [`proc_macro2::LexError`], which is not Send.
#[derive(Debug, Display)]
#[display("{_0}")]
pub struct TokenStreamParseError(String);

impl std::error::Error for TokenStreamParseError {}

impl std::error::Error for CordialError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::JsonParse { err, .. } => Some(err),
            Self::Config(err) => Some(err),
            Self::SynParse { err, .. } => Some(err),
            Self::CargoMetadata(err) => Some(err),
            Self::Fmt(err) => Some(err),
            Self::TokenStreamParse(err) => Some(err),
            Self::Invariant { .. } | Self::UnknownEtiquette { .. } => None,
        }
    }
}

pub type CordialResult<T> = Result<T, CordialError>;

impl From<std::io::Error> for CordialError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for CordialError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<config::ConfigError> for CordialError {
    fn from(value: config::ConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<cargo_metadata::Error> for CordialError {
    fn from(value: cargo_metadata::Error) -> Self {
        Self::CargoMetadata(value)
    }
}

impl From<std::fmt::Error> for CordialError {
    fn from(value: std::fmt::Error) -> Self {
        Self::Fmt(value)
    }
}

impl From<proc_macro2::LexError> for CordialError {
    fn from(value: proc_macro2::LexError) -> Self {
        Self::TokenStreamParse(TokenStreamParseError(value.to_string()))
    }
}

impl CordialError {
    pub fn invariant(message: impl Into<String>) -> Self {
        Self::Invariant {
            message: message.into(),
        }
    }

    pub fn syn_parse(path: impl Into<String>, err: syn::Error) -> Self {
        Self::SynParse {
            path: path.into(),
            err,
        }
    }

    pub fn json_parse(path: impl Into<String>, err: serde_json::Error) -> Self {
        Self::JsonParse {
            path: path.into(),
            err,
        }
    }

    pub fn cargo_metadata(err: cargo_metadata::Error) -> Self {
        Self::from(err)
    }
}
