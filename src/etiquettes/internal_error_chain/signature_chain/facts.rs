//! Closed signature grammar retained in shared IR, independent of parser nodes.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub(super) struct Path {
    parts: Vec<String>,
    args: Option<Vec<Type>>,
    #[getter(copy)]
    simple: bool,
}

impl Path {
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn ident(&self) -> Option<&str> {
        self.simple
            .then(|| self.parts.first().map(String::as_str))
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Type {
    Path(Path),
    Transparent(Box<Type>),
    Tuple(Vec<Type>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Expr {
    Path(Path),
    Call { path: Option<Path>, args: Vec<Expr> },
    Transparent(Box<Expr>),
    Unsupported,
}
