//! Compatibility projection from the loader's shared parse.

use super::{ApplyUses, LocatedName};
use crate::loader::SharedParsedFile;
use syn::visit::Visit;

impl ApplyUses {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ApplyUses")]
    pub(in crate::ir::error_apply_source) fn collect(file: &SharedParsedFile) -> Self {
        let mut collector = Collector {
            facts: Self::empty(file.source()),
            module_depth: 0,
        };
        collector.visit_file(file.syntax());
        collector.facts.normalize();
        collector.facts
    }
}

struct Collector {
    facts: ApplyUses,
    module_depth: usize,
}

impl<'ast> Visit<'ast> for Collector {
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.facts.declarations.insert(located(&item.ident));
        syn::visit::visit_item_struct(self, item);
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.facts.declarations.insert(located(&item.ident));
        syn::visit::visit_item_enum(self, item);
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_attribute(&mut self, item: &'ast syn::Attribute) {
        self.facts.has_cfg |= item.path().is_ident("cfg") || item.path().is_ident("cfg_attr");
        syn::visit::visit_attribute(self, item);
    }

    #[tracing::instrument(level = "trace", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        self.facts.identifiers.insert(located(ident));
    }

    #[tracing::instrument(level = "trace", skip(self, path))]
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.module_depth == 0 {
            let mut prefix = Vec::new();
            for segment in &path.segments {
                prefix.push(segment.ident.to_string());
                self.facts
                    .paths
                    .push((located(&segment.ident), prefix.clone()));
            }
        }
        syn::visit::visit_path(self, path);
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        collect_token_idents(&item.tokens, &mut self.facts.macro_names);
        syn::visit::visit_macro(self, item);
    }
}

#[tracing::instrument(level = "trace", skip(ident))]
fn located(ident: &proc_macro2::Ident) -> LocatedName {
    let start = ident.span().start();
    (ident.to_string(), start.line, start.column)
}

#[tracing::instrument(level = "trace", skip(tokens, names))]
fn collect_token_idents(
    tokens: &proc_macro2::TokenStream,
    names: &mut std::collections::BTreeSet<String>,
) {
    for tree in tokens.clone() {
        match tree {
            proc_macro2::TokenTree::Ident(ident) => {
                names.insert(ident.to_string());
            }
            proc_macro2::TokenTree::Group(group) => collect_token_idents(&group.stream(), names),
            _ => {}
        }
    }
}
