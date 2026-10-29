//! Decline generated API changes when a getter consumer is unresolved.

use std::collections::BTreeSet;

use syn::visit::Visit;
use tracing::instrument;

use super::bindings::{BindingCounts, caller_aliases};
use super::replacements::is_location_caller;

/// Location::file calls have a known contract. Other file getters need
/// receiver and consumer evidence before their return storage can change.
#[instrument(level = "trace", skip(syntax))]
pub(in crate::etiquettes::internal_error_chain::apply) fn has_unresolved_getter_uses(
    syntax: &syn::File,
) -> bool {
    let mut visitor = GetterUses::default();
    visitor.visit_file(syntax);
    visitor.unresolved
}

#[derive(Default)]
struct GetterUses {
    aliases: BTreeSet<String>,
    unresolved: bool,
}

impl GetterUses {
    #[instrument(level = "trace", skip(self, block))]
    fn body(&mut self, block: &syn::Block) {
        let mut bindings = BindingCounts::default();
        bindings.visit_block(block);
        let previous = std::mem::replace(&mut self.aliases, caller_aliases(block, &bindings));
        syn::visit::visit_block(self, block);
        self.aliases = previous;
    }

    #[instrument(level = "trace", skip(self, tokens))]
    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Ident(name) if name == "file" && index > 0 => {
                    self.unresolved |= matches!(&tokens[index - 1],
                        proc_macro2::TokenTree::Punct(punct) if matches!(punct.as_char(), '.' | ':'));
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for GetterUses {
    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.body(&item.block);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.body(&item.block);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_closure(&mut self, item: &'ast syn::ExprClosure) {
        let previous = std::mem::take(&mut self.aliases);
        syn::visit::visit_expr_closure(self, item);
        self.aliases = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        if item.method == "file" {
            let known = match &*item.receiver {
                syn::Expr::Path(path) => {
                    path.qself.is_none()
                        && path
                            .path
                            .get_ident()
                            .is_some_and(|name| self.aliases.contains(&name.to_string()))
                }
                syn::Expr::Call(call) => is_location_caller(call),
                _ => false,
            };
            self.unresolved |= !known;
        }
        syn::visit::visit_expr_method_call(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        self.unresolved |= item.path.segments.len() > 1
            && item
                .path
                .segments
                .last()
                .is_some_and(|part| part.ident == "file");
        syn::visit::visit_expr_path(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.tokens(item.tokens.clone());
    }
}
