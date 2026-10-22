//! Constructor initializer recognition for conservative error field edits.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ExprStruct, ImplItemFn, ItemFn, ItemImpl, Member, Signature};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::{Edit, simple_type_name, span_range};

mod bindings;
mod replacements;

use bindings::{
    BindingCounts, caller_aliases, impl_uses_lifetime, static_string_locals, static_string_params,
};
use replacements::{file_initializer_replacement, static_string_initializer_replacement};

pub(super) struct InitializerVisitor<'a> {
    type_name: &'a str,
    field_name: &'a str,
    kind: FieldKind,
    strategy: ErrorFieldStrategy,
    source: &'a str,
    static_strings: &'a BTreeSet<String>,
    target_lifetime: Option<&'a str>,
    current_impl: Option<String>,
    current_impl_lifetime_matches: bool,
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    static_locals: BTreeMap<String, usize>,
    edits: Vec<Edit>,
    found: usize,
    unresolved: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FieldKind {
    CallSiteFile,
    StaticString,
}

impl<'a> InitializerVisitor<'a> {
    #[instrument(level = "debug", skip(kind, strategy, source, static_strings))]
    pub(super) fn new(
        type_name: &'a str,
        field_name: &'a str,
        kind: FieldKind,
        strategy: ErrorFieldStrategy,
        source: &'a str,
        static_strings: &'a BTreeSet<String>,
        target_lifetime: Option<&'a str>,
    ) -> Self {
        Self {
            type_name,
            field_name,
            kind,
            strategy,
            source,
            static_strings,
            target_lifetime,
            current_impl: None,
            current_impl_lifetime_matches: false,
            caller_aliases: BTreeSet::new(),
            static_params: BTreeSet::new(),
            static_locals: BTreeMap::new(),
            edits: Vec::new(),
            found: 0,
            unresolved: false,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn finish(self) -> Option<Vec<Edit>> {
        (self.found > 0 && !self.unresolved).then_some(self.edits)
    }

    #[instrument(level = "debug", skip(self, signature, block))]
    fn visit_constructor_block(&mut self, signature: &Signature, block: &syn::Block) {
        let mut bindings = BindingCounts::default();
        bindings.visit_block(block);
        let previous =
            std::mem::replace(&mut self.caller_aliases, caller_aliases(block, &bindings));
        let previous_params = std::mem::replace(
            &mut self.static_params,
            static_string_params(
                signature,
                &bindings,
                self.target_lifetime,
                self.current_impl_lifetime_matches,
            ),
        );
        let previous_locals = std::mem::replace(
            &mut self.static_locals,
            static_string_locals(
                signature,
                block,
                &bindings,
                self.source,
                self.target_lifetime,
                self.current_impl_lifetime_matches,
            ),
        );
        syn::visit::visit_block(self, block);
        self.caller_aliases = previous;
        self.static_params = previous_params;
        self.static_locals = previous_locals;
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
            self.target_lifetime
                .is_some_and(|lifetime| impl_uses_lifetime(item, self.type_name, lifetime)),
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
        let target = item.path.is_ident(self.type_name)
            || (item.path.is_ident("Self") && self.current_impl.as_deref() == Some(self.type_name));
        if !target
            && item
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.type_name)
        {
            self.unresolved = true;
        }
        if target {
            self.found += 1;
            if item.rest.is_some() {
                self.unresolved = true;
            } else if let Some(field) = item.fields.iter().find(
                |field| matches!(&field.member, Member::Named(name) if name == self.field_name),
            ) {
                let replacement = match self.kind {
                    FieldKind::CallSiteFile => file_initializer_replacement(
                        self.source,
                        &field.expr,
                        self.strategy,
                        &self.caller_aliases,
                    ),
                    FieldKind::StaticString => static_string_initializer_replacement(
                        self.source,
                        &field.expr,
                        self.strategy,
                        self.static_strings,
                        &self.static_params,
                        &self.static_locals,
                    ),
                };
                if let (Some((start, end)), Some(replacement)) =
                    (span_range(self.source, field.expr.span()), replacement)
                {
                    let replacement = if field.colon_token.is_none() {
                        format!("{}: {replacement}", self.field_name)
                    } else {
                        replacement
                    };
                    self.edits.push(Edit {
                        start,
                        end,
                        replacement,
                    });
                } else {
                    self.unresolved = true;
                }
            } else {
                self.unresolved = true;
            }
        }
        syn::visit::visit_expr_struct(self, item);
    }
}
