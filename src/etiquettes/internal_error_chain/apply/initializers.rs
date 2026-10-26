//! Constructor initializer recognition for conservative error field edits.

use std::collections::{BTreeMap, BTreeSet};

use syn::visit::Visit;
use syn::{ExprStruct, ImplItemFn, ItemFn, ItemImpl, Member, Signature};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::{Edit, simple_type_name};

mod batch;
mod bindings;
mod replacements;

pub(super) use batch::{InitializerBatchInput, collect_initializer_facts};
pub(super) use bindings::impl_uses_lifetime;
use bindings::{BindingCounts, caller_aliases, static_string_locals, static_string_params};
use replacements::{InitializerExpressionFact, inspect_initializer};

/// Constructor sites and provenance recorded in the shared IR before apply.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct InitializerFacts {
    sites: Vec<Option<InitializerExpressionFact>>,
    found: usize,
    unresolved: bool,
}

impl InitializerFacts {
    #[instrument(level = "trace", skip(self, source, strategy))]
    pub(super) fn edits(&self, source: &str, strategy: ErrorFieldStrategy) -> Option<Vec<Edit>> {
        (self.found > 0 && !self.unresolved)
            .then(|| {
                self.sites
                    .iter()
                    .map(|site| site.as_ref()?.edit(source, strategy))
                    .collect()
            })
            .flatten()
    }
}

pub(super) struct InitializerVisitor<'a> {
    type_name: &'a str,
    field_name: &'a str,
    kind: FieldKind,
    source: &'a str,
    static_strings: &'a BTreeSet<String>,
    target_lifetime: Option<&'a str>,
    pending_lifetime: bool,
    current_impl: Option<String>,
    current_impl_lifetime_matches: bool,
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    static_locals: BTreeMap<String, usize>,
    facts: InitializerFacts,
}

struct ConstructorContext {
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    static_locals: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum FieldKind {
    CallSiteFile,
    StaticString,
}

impl<'a> InitializerVisitor<'a> {
    #[instrument(level = "debug", skip(kind, source, static_strings))]
    pub(super) fn new(
        type_name: &'a str,
        field_name: &'a str,
        kind: FieldKind,
        source: &'a str,
        static_strings: &'a BTreeSet<String>,
        target_lifetime: Option<&'a str>,
        pending_lifetime: bool,
    ) -> Self {
        Self {
            type_name,
            field_name,
            kind,
            source,
            static_strings,
            target_lifetime,
            pending_lifetime,
            current_impl: None,
            current_impl_lifetime_matches: false,
            caller_aliases: BTreeSet::new(),
            static_params: BTreeSet::new(),
            static_locals: BTreeMap::new(),
            facts: InitializerFacts::default(),
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn finish(self) -> InitializerFacts {
        self.facts
    }

    #[instrument(level = "debug", skip(self, signature, block))]
    fn visit_constructor_block(&mut self, signature: &Signature, block: &syn::Block) {
        let mut bindings = BindingCounts::default();
        bindings.visit_block(block);
        let aliases = caller_aliases(block, &bindings);
        let previous = self.enter_constructor_block(signature, block, &bindings, &aliases);
        syn::visit::visit_block(self, block);
        self.leave_constructor_block(previous);
    }

    #[instrument(level = "debug", skip(self, signature, block, bindings, aliases))]
    fn enter_constructor_block(
        &mut self,
        signature: &Signature,
        block: &syn::Block,
        bindings: &BindingCounts,
        aliases: &BTreeSet<String>,
    ) -> ConstructorContext {
        let caller_aliases = std::mem::replace(&mut self.caller_aliases, aliases.clone());
        let static_params = std::mem::replace(
            &mut self.static_params,
            static_string_params(
                signature,
                bindings,
                self.target_lifetime,
                self.current_impl_lifetime_matches,
            ),
        );
        let static_locals = std::mem::replace(
            &mut self.static_locals,
            static_string_locals(
                signature,
                block,
                bindings,
                self.source,
                self.target_lifetime,
                self.current_impl_lifetime_matches,
            ),
        );
        ConstructorContext {
            caller_aliases,
            static_params,
            static_locals,
        }
    }

    #[instrument(level = "debug", skip(self, previous))]
    fn leave_constructor_block(&mut self, previous: ConstructorContext) {
        self.caller_aliases = previous.caller_aliases;
        self.static_params = previous.static_params;
        self.static_locals = previous.static_locals;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn inspect_expr_struct(&mut self, item: &ExprStruct) {
        let target = item.path.is_ident(self.type_name)
            || (item.path.is_ident("Self") && self.current_impl.as_deref() == Some(self.type_name));
        if !target
            && item
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.type_name)
        {
            self.facts.unresolved = true;
        }
        if target {
            self.facts.found += 1;
            if item.rest.is_some() {
                self.facts.unresolved = true;
            } else if let Some(field) = item.fields.iter().find(
                |field| matches!(&field.member, Member::Named(name) if name == self.field_name),
            ) {
                self.facts.sites.push(inspect_initializer(
                    self.source,
                    &field.expr,
                    self.field_name,
                    field.colon_token.is_none(),
                    self.kind,
                    &self.caller_aliases,
                    self.static_strings,
                    &self.static_params,
                    &self.static_locals,
                ));
            } else {
                self.facts.unresolved = true;
            }
        }
    }
}

impl<'ast> Visit<'ast> for InitializerVisitor<'_> {
    #[instrument(level = "debug", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {
        // A nested module has its own `Self` and privacy context.
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let previous = std::mem::replace(&mut self.current_impl, simple_type_name(&item.self_ty));
        let previous_lifetime = std::mem::replace(
            &mut self.current_impl_lifetime_matches,
            self.target_lifetime.is_some_and(|lifetime| {
                impl_uses_lifetime(item, self.type_name, lifetime)
                    || (self.pending_lifetime
                        && simple_type_name(&item.self_ty).as_deref() == Some(self.type_name))
            }),
        );
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
        self.current_impl_lifetime_matches = previous_lifetime;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        let previous = std::mem::replace(&mut self.current_impl_lifetime_matches, false);
        self.visit_constructor_block(&item.sig, &item.block);
        self.current_impl_lifetime_matches = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.visit_constructor_block(&item.sig, &item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        self.inspect_expr_struct(item);
        syn::visit::visit_expr_struct(self, item);
    }
}
