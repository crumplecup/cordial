//! Select Verus invocations from the compatibility shared Rust syntax.
use super::{Items, VerusBlock};
use std::path::Path;
use syn::visit::Visit;
use tracing::instrument;

#[instrument(level = "debug", skip(syntax, file))]
pub(in crate::verus_ir) fn blocks_in_syntax(
    syntax: &syn::File,
    file: &Path,
    module_path: &str,
) -> Vec<VerusBlock> {
    let mut finder = VerusMacroFinder::default();
    finder.visit_file(syntax);
    finder
        .macros
        .into_iter()
        .filter_map(|found| {
            let parsed = verus_syn::parse2::<Items>(found.tokens).ok()?;
            Some(VerusBlock {
                file: file.to_path_buf(),
                module_path: module_path.to_owned(),
                cfg_test: found.cfg_test,
                items: parsed.items,
            })
        })
        .collect()
}

/// One `verus! { .. }` macro invocation found, with whether it sits
/// inside a `#[cfg(test)]` module.
struct FoundMacro {
    tokens: proc_macro2::TokenStream,
    cfg_test: bool,
}

/// Collects every `verus! { .. }` macro invocation in a file, at item,
/// statement, or expression position (in practice always item position
/// in this codebase, but all three are real, valid places a macro
/// invocation can appear), tracking `#[cfg(test)]` module nesting the
/// same way `panics::scan::PanicScanVisitor` does.
#[derive(Default)]
struct VerusMacroFinder {
    in_cfg_test: bool,
    macros: Vec<FoundMacro>,
}

impl VerusMacroFinder {
    #[instrument(level = "trace", skip(self, mac))]
    fn check(&mut self, mac: &syn::Macro) {
        if mac.path.is_ident("verus") {
            self.macros.push(FoundMacro {
                tokens: mac.tokens.clone(),
                cfg_test: self.in_cfg_test,
            });
        }
    }
}

impl<'ast> Visit<'ast> for VerusMacroFinder {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_cfg_test = prev;
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        self.check(&node.mac);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.check(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        self.check(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }
}

/// Whether `attrs` carries a bare `#[cfg(test)]`.
#[instrument(level = "trace", skip(attrs), ret)]
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == "test"
    })
}
