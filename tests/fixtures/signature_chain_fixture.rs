use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct SourceError;

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source")
    }
}
impl Error for SourceError {}

#[derive(Debug)]
pub struct MessageError(String);

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Error for MessageError {}

#[derive(Debug)]
pub struct ChainError {
    source: SourceError,
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "chain")
    }
}
impl Error for ChainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub struct FakeChainError {
    source: SourceError,
}

impl fmt::Display for FakeChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fake chain")
    }
}
impl Error for FakeChainError {}

#[derive(Debug)]
pub enum BroadError {
    Wrapped(SourceError),
    Message(String),
}

impl fmt::Display for BroadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "broad")
    }
}
impl Error for BroadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wrapped(source) => Some(source),
            Self::Message(_) => None,
        }
    }
}

pub mod helpers {
    use super::{BroadError, ChainError, FakeChainError, MessageError, SourceError};

    pub fn drop_source(err: SourceError) -> MessageError {
        MessageError(err.to_string())
    }
    pub fn forward(err: SourceError) -> SourceError {
        err
    }
    pub fn drop_later(err: SourceError) -> MessageError {
        MessageError(err.to_string())
    }
    pub fn drop_generic<E: std::error::Error>(err: E) -> MessageError {
        MessageError(err.to_string())
    }
    pub fn preserve_source(err: SourceError) -> ChainError {
        ChainError { source: err }
    }
    pub fn fake_source(err: SourceError) -> FakeChainError {
        FakeChainError { source: err }
    }
    pub fn broad(err: SourceError) -> BroadError {
        BroadError::Message(err.to_string())
    }
}

pub fn single(result: Result<(), SourceError>) -> Result<(), MessageError> {
    result.map_err(|err| helpers::drop_source(err))
}

pub fn two_calls(result: Result<(), SourceError>) -> Result<(), MessageError> {
    result.map_err(|err| helpers::drop_later(helpers::forward(err)))
}

pub fn preserved(result: Result<(), SourceError>) -> Result<(), SourceError> {
    result.map_err(|err| helpers::forward(err))
}

pub fn wrapped(result: Result<(), SourceError>) -> Result<(), ChainError> {
    result.map_err(|err| helpers::preserve_source(err))
}

pub fn fake_wrapped(result: Result<(), SourceError>) -> Result<(), FakeChainError> {
    result.map_err(|err| helpers::fake_source(err))
}

pub fn external_call(result: Result<(), SourceError>) -> Result<(), std::io::Error> {
    result.map_err(|err| std::io::Error::other(err))
}

pub fn generic_call(result: Result<(), SourceError>) -> Result<(), MessageError> {
    result.map_err(|err| helpers::drop_generic(err))
}

pub fn broad_call(result: Result<(), SourceError>) -> Result<(), BroadError> {
    result.map_err(|err| helpers::broad(err))
}
