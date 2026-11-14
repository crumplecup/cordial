//! Information about the target.

use std::fmt;

use triomphe::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Arch {
    // Only what we need is present here.
    Wasm32,
    Wasm64,
    Other,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TargetData {
    pub data_layout: Box<str>,
    pub arch: Arch,
}

#[derive(Clone, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(from = "String", into = "String")]
pub struct TargetLoadError(Arc<str>);

impl From<TargetLoadError> for String {
    fn from(error: TargetLoadError) -> Self {
        error.0.to_string()
    }
}

impl fmt::Debug for TargetLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for TargetLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for TargetLoadError {}

impl From<String> for TargetLoadError {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl From<&str> for TargetLoadError {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

pub type TargetLoadResult = Result<TargetData, TargetLoadError>;
