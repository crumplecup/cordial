//! Lexical return-signature facts for `?` expressions.

use syn::{GenericArgument, PathArguments, ReturnType, Type};
use tracing::instrument;

use super::helpers::plain_type_path;

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
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub struct ErrorTryEvidence {
    #[getter(copy)]
    scope: ErrorTryScope,
    return_type_path: Option<Vec<String>>,
    error_type_path: Option<Vec<String>>,
}

impl ErrorTryEvidence {
    #[instrument(level = "debug", skip(output, scope))]
    pub(super) fn for_scope(output: Option<&ReturnType>, scope: ErrorTryScope) -> Self {
        let path = output.and_then(|output| match output {
            ReturnType::Type(_, ty) => match &**ty {
                Type::Path(path) if path.qself.is_none() => Some(&path.path),
                _ => None,
            },
            ReturnType::Default => None,
        });
        let return_type_path = path.map(|path| {
            path.segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect()
        });
        let error_type_path = path
            .and_then(|path| path.segments.last())
            .filter(|part| part.ident == "Result")
            .and_then(|part| match &part.arguments {
                PathArguments::AngleBracketed(args) if args.args.len() == 2 => args.args.last(),
                _ => None,
            })
            .and_then(|argument| match argument {
                GenericArgument::Type(ty) => plain_type_path(ty),
                _ => None,
            });
        Self {
            scope,
            return_type_path,
            error_type_path,
        }
    }
}
