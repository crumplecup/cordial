//! Lexical return-signature facts for `?` expressions.

/// The enclosing expression scope that determines the meaning of `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorTryScope {
    /// A free or associated function with an explicit return signature.
    Function,
    /// A closure with an explicit return signature.
    Closure,
    /// A closure whose return type is inferred.
    InferredClosure,
    /// An async block whose output type is inferred.
    AsyncBlock,
    /// A try block whose output type is inferred.
    TryBlock,
}

/// Written return type and possible `Result` error type at a `?` site.
///
/// These are syntax facts only. The operand's error type and the identity of
/// either written path still require type resolution.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_getters::Getters,
    derive_new::new,
)]
pub struct ErrorTryEvidence {
    #[getter(copy)]
    scope: ErrorTryScope,
    return_type_path: Option<Vec<String>>,
    error_type_path: Option<Vec<String>>,
}
