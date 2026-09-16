//! Clause-level exemptions that do not need registry lookup.

use proc_macro2::TokenStream;
use tracing::instrument;

/// A bare `true`/`false` clause is tautological; a bare `result`, tuple
/// projection of `result`, or `result.N is None` delegates the real claim
/// to the verifier function body rather than spelling a relationship the
/// scanner should require to be named.
#[instrument(level = "trace", ret)]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn is_trivial(normalized: &str) -> bool {
    normalized == "true"
        || normalized == "false"
        || normalized == "result"
        || normalized == "! result"
        || is_bare_result_projection(normalized)
        || is_bare_result_is_none(normalized)
}

/// Whether `normalized` is exactly `result . N` or `! result . N` for
/// some decimal tuple index `N` -- nothing else appended.
#[instrument(level = "trace", ret)]
fn is_bare_result_projection(normalized: &str) -> bool {
    let rest = normalized.strip_prefix("! ").unwrap_or(normalized);
    rest.strip_prefix("result . ")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `normalized` is exactly `result . N is None` for some decimal
/// tuple index `N` -- nothing else appended.
#[instrument(level = "trace", ret)]
fn is_bare_result_is_none(normalized: &str) -> bool {
    normalized
        .strip_prefix("result . ")
        .and_then(|rest| rest.strip_suffix(" is None"))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
}

/// Verus's builtin function-item contract-inspection syntax is a real
/// method call on an external function item, not a project-local
/// predicate the registry could name.
#[instrument(level = "trace", skip(clause), ret)]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn is_builtin_contract_inspection(
    kind: &str,
    clause: TokenStream,
) -> bool {
    let Ok(syn::Expr::MethodCall(method_call)) = syn::parse2::<syn::Expr>(clause) else {
        return false;
    };
    method_call.method == kind
}
