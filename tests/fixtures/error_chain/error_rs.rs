
use std::error::Error;

#[derive(Debug)]
enum CordialError {
    Io(std::io::Error),
    SynParse { path: String, err: syn::Error },
    JsonParse { path: String, err: serde_json::Error },
    Invariant { message: String },
    CargoMetadata(cargo_metadata::Error),
}

impl Error for CordialError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::SynParse { err, .. } => Some(err),
            Self::JsonParse { err, .. } => Some(err),
            Self::CargoMetadata(err) => Some(err),
            Self::Invariant { .. } => None,
        }
    }
}
