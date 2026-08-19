//! Shared shape helpers for native-source constructors and location fields.

use syn::visit::Visit;
use syn::{Attribute, FnArg, ReturnType, Signature, Type};

use super::type_graph::type_label;

use tracing::instrument;
#[instrument(level = "trace", skip(attrs), ret)]
pub(crate) fn has_track_caller(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("track_caller"))
}

#[instrument(level = "debug", skip(ty))]
pub(crate) fn type_is_location(ty: &Type) -> bool {
    type_label(ty)
        .rsplit("::")
        .next()
        .is_some_and(|last| last == "Location")
}

#[instrument(level = "debug")]
pub(crate) fn type_labels_match(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!("::{b}")) || b.ends_with(&format!("::{a}"))
}

#[instrument(level = "debug", skip(sig))]
pub(crate) fn sig_takes_location_arg(sig: &Signature) -> bool {
    sig.inputs.iter().any(|arg| {
        let FnArg::Typed(pat) = arg else {
            return false;
        };
        if type_is_location(&pat.ty) {
            return true;
        }
        let syn::Pat::Ident(ident) = pat.pat.as_ref() else {
            return false;
        };
        matches!(
            ident.ident.to_string().as_str(),
            "file" | "line" | "location"
        )
    })
}

#[instrument(level = "debug", skip(sig))]
pub(crate) fn returns_self(sig: &Signature, type_name: &str) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => {
            let label = type_label(ty);
            label == "Self" || label == type_name || label.ends_with(&format!("::{type_name}"))
        }
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(block))]
pub(crate) fn block_captures_location(block: &syn::Block) -> bool {
    let mut hunt = LocationCallerHunt { found: false };
    hunt.visit_block(block);
    hunt.found
}

struct LocationCallerHunt {
    found: bool,
}

impl<'ast> Visit<'ast> for LocationCallerHunt {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        let owned: Vec<String> = node
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if owned
            .windows(2)
            .any(|window| window[0] == "Location" && window[1] == "caller")
        {
            self.found = true;
        }
        syn::visit::visit_expr_path(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "caller" {
            self.found = true;
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}
