//! Collect initializer evidence for every candidate in one source-tree walk.

use std::collections::BTreeSet;

use syn::visit::Visit;
use syn::{ExprStruct, ImplItemFn, ItemFn, ItemImpl, Signature};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::bindings::{BindingCounts, caller_aliases, impl_uses_lifetime};
use super::{
    ConstructorContext, FieldKind, InitializerFacts, InitializerVisitor, simple_type_name,
    tuple_file_origins,
};

#[derive(derive_new::new)]
pub(crate) struct InitializerBatchInput<'a> {
    type_name: &'a str,
    field_name: &'a str,
    kind: FieldKind,
    lifetime: Option<&'a str>,
    pending_lifetime: bool,
    cfg_attrs: &'a [String],
}

struct BatchInitializerVisitor<'a> {
    candidates: Vec<InitializerVisitor<'a>>,
}

#[instrument(level = "debug", skip(file, inputs, static_strings))]
pub(crate) fn collect_initializer_facts<'a>(
    file: &'a SharedParsedFile,
    inputs: &[InitializerBatchInput<'a>],
    static_strings: &'a BTreeSet<String>,
) -> Vec<InitializerFacts> {
    let mut visitor = BatchInitializerVisitor {
        candidates: inputs
            .iter()
            .map(|input| {
                InitializerVisitor::new(
                    input.type_name,
                    input.field_name,
                    input.kind,
                    file.source(),
                    static_strings,
                    input.lifetime,
                    input.pending_lifetime,
                    input.cfg_attrs,
                )
            })
            .collect(),
    };
    visitor.visit_file(file.syntax());
    visitor
        .candidates
        .into_iter()
        .map(InitializerVisitor::finish)
        .collect()
}

impl BatchInitializerVisitor<'_> {
    #[instrument(level = "debug", skip(self, signature, block))]
    fn visit_constructor_block(&mut self, signature: &Signature, block: &syn::Block) {
        let mut bindings = BindingCounts::default();
        bindings.visit_block(block);
        let aliases = caller_aliases(block, &bindings);
        let tuple_files = self
            .candidates
            .first()
            .map(|candidate| tuple_file_origins(candidate.source, block, &bindings))
            .unwrap_or_default();
        let previous: Vec<ConstructorContext> = self
            .candidates
            .iter_mut()
            .map(|candidate| {
                candidate.enter_constructor_block(
                    signature,
                    block,
                    &bindings,
                    &aliases,
                    &tuple_files,
                )
            })
            .collect();
        syn::visit::visit_block(self, block);
        for (candidate, context) in self.candidates.iter_mut().zip(previous) {
            candidate.leave_constructor_block(context);
        }
    }
}

impl<'ast> Visit<'ast> for BatchInitializerVisitor<'_> {
    #[instrument(level = "debug", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {
        // A nested module has a different privacy and `Self` context.
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let current = simple_type_name(&item.self_ty);
        let previous: Vec<_> = self
            .candidates
            .iter_mut()
            .map(|candidate| {
                let prior_impl = std::mem::replace(&mut candidate.current_impl, current.clone());
                let prior_lifetime = std::mem::replace(
                    &mut candidate.current_impl_lifetime_matches,
                    candidate.target_lifetime.is_some_and(|lifetime| {
                        impl_uses_lifetime(item, candidate.type_name, lifetime)
                            || (candidate.pending_lifetime
                                && current.as_deref() == Some(candidate.type_name))
                    }),
                );
                (prior_impl, prior_lifetime)
            })
            .collect();
        syn::visit::visit_item_impl(self, item);
        for (candidate, (prior_impl, prior_lifetime)) in self.candidates.iter_mut().zip(previous) {
            candidate.current_impl = prior_impl;
            candidate.current_impl_lifetime_matches = prior_lifetime;
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        let prior: Vec<_> = self
            .candidates
            .iter_mut()
            .map(|candidate| std::mem::replace(&mut candidate.current_impl_lifetime_matches, false))
            .collect();
        self.visit_constructor_block(&item.sig, &item.block);
        for (candidate, prior_lifetime) in self.candidates.iter_mut().zip(prior) {
            candidate.current_impl_lifetime_matches = prior_lifetime;
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.visit_constructor_block(&item.sig, &item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        for candidate in &mut self.candidates {
            candidate.inspect_expr_struct(item);
        }
        syn::visit::visit_expr_struct(self, item);
    }
}
