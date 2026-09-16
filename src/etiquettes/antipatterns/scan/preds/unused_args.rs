//! Pattern predicates for underscore-prefixed unused parameters.

use syn::Pat;
use syn::spanned::Spanned;

use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub(in crate::etiquettes::antipatterns::scan) struct UnusedArgBinding {
    #[getter(copy)]
    line: u32,
    snippet: String,
}

#[instrument(level = "debug", skip(pat))]
pub(in crate::etiquettes::antipatterns::scan) fn unused_argument_bindings(
    pat: &Pat,
) -> Vec<UnusedArgBinding> {
    let mut bindings = Vec::new();
    collect_unused_argument_bindings(pat, &mut bindings);
    bindings
}

#[instrument(level = "debug", skip(pat, bindings))]
fn collect_unused_argument_bindings(pat: &Pat, bindings: &mut Vec<UnusedArgBinding>) {
    match pat {
        Pat::Wild(_) => {}
        Pat::Ident(ident) if is_unused_argument_ident(&ident.ident) => {
            bindings.push(UnusedArgBinding::new(
                ident.span().start().line as u32,
                ident.ident.to_string(),
            ));
        }
        Pat::Reference(reference) => collect_unused_argument_bindings(&reference.pat, bindings),
        Pat::Type(pat_type) => collect_unused_argument_bindings(&pat_type.pat, bindings),
        Pat::Paren(paren) => collect_unused_argument_bindings(&paren.pat, bindings),
        Pat::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::TupleStruct(tuple_struct) => {
            for element in &tuple_struct.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::Struct(pat_struct) => {
            for field in &pat_struct.fields {
                collect_unused_argument_bindings(&field.pat, bindings);
            }
        }
        _ => {}
    }
}

#[instrument(level = "trace", skip(ident), ret)]
fn is_unused_argument_ident(ident: &syn::Ident) -> bool {
    ident.to_string().starts_with('_')
}
