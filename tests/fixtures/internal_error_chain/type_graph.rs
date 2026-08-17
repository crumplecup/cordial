
use std::error::Error;

#[derive(Debug)]
enum DomainError {
    Invariant { detail: String },
    Wrapped { source: InnerSource },
}

#[derive(Debug)]
struct InnerSource {
    source: std::io::Error,
}

impl Error for DomainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wrapped { source } => Some(source),
            Self::Invariant { .. } => None,
        }
    }
}
