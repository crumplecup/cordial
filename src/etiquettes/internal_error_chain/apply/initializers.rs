//! Constructor initializer recognition for conservative error field edits.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprStruct, ImplItemFn, ItemFn, ItemImpl, Member, Pat, Signature};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::{Edit, simple_type_name, span_range};

mod batch;
mod bindings;
mod getter_uses;
mod replacements;

pub(super) use batch::{InitializerBatchInput, collect_initializer_facts};
pub(super) use bindings::impl_uses_lifetime;
use bindings::{BindingCounts, caller_aliases, static_string_locals, static_string_params};
pub(super) use getter_uses::has_unresolved_getter_uses;
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
    cfg_attrs: &'a [String],
    current_impl: Option<String>,
    current_impl_lifetime_matches: bool,
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    static_locals: BTreeMap<String, usize>,
    tuple_files: BTreeMap<String, TupleFileOrigin>,
    facts: InitializerFacts,
}

struct ConstructorContext {
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    static_locals: BTreeMap<String, usize>,
    tuple_files: BTreeMap<String, TupleFileOrigin>,
}

#[derive(Clone, Debug)]
struct TupleFileOrigin {
    fact: InitializerExpressionFact,
    cfg: Vec<String>,
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
        cfg_attrs: &'a [String],
    ) -> Self {
        Self {
            type_name,
            field_name,
            kind,
            source,
            static_strings,
            target_lifetime,
            pending_lifetime,
            cfg_attrs,
            current_impl: None,
            current_impl_lifetime_matches: false,
            caller_aliases: BTreeSet::new(),
            static_params: BTreeSet::new(),
            static_locals: BTreeMap::new(),
            tuple_files: BTreeMap::new(),
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
        let tuple_files = tuple_file_origins(self.source, block, &bindings);
        let previous =
            self.enter_constructor_block(signature, block, &bindings, &aliases, &tuple_files);
        syn::visit::visit_block(self, block);
        self.leave_constructor_block(previous);
    }

    #[instrument(
        level = "debug",
        skip(self, signature, block, bindings, aliases, tuple_files)
    )]
    fn enter_constructor_block(
        &mut self,
        signature: &Signature,
        block: &syn::Block,
        bindings: &BindingCounts,
        aliases: &BTreeSet<String>,
        tuple_files: &BTreeMap<String, TupleFileOrigin>,
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
        let previous_tuple_files = std::mem::replace(&mut self.tuple_files, tuple_files.clone());
        ConstructorContext {
            caller_aliases,
            static_params,
            static_locals,
            tuple_files: previous_tuple_files,
        }
    }

    #[instrument(level = "debug", skip(self, previous))]
    fn leave_constructor_block(&mut self, previous: ConstructorContext) {
        self.caller_aliases = previous.caller_aliases;
        self.static_params = previous.static_params;
        self.static_locals = previous.static_locals;
        self.tuple_files = previous.tuple_files;
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
                if cfg_text(self.source, &field.attrs).as_deref() != Some(self.cfg_attrs) {
                    self.facts.unresolved = true;
                    return;
                }
                let direct = inspect_initializer(
                    self.source,
                    &field.expr,
                    self.field_name,
                    field.colon_token.is_none(),
                    self.kind,
                    &self.caller_aliases,
                    self.static_strings,
                    &self.static_params,
                    &self.static_locals,
                );
                let tuple = match &field.expr {
                    Expr::Path(path)
                        if self.kind == FieldKind::CallSiteFile && path.qself.is_none() =>
                    {
                        let name = path.path.get_ident().map(ToString::to_string);
                        name.and_then(|name| self.tuple_files.get(&name))
                            .filter(|origin| {
                                cfg_text(self.source, &field.attrs).as_ref() == Some(&origin.cfg)
                            })
                            .filter(|origin| {
                                span_range(self.source, field.expr.span())
                                    .is_some_and(|(start, _)| origin.fact.end() < start)
                            })
                            .map(|origin| origin.fact.clone())
                    }
                    _ => None,
                };
                self.facts.sites.push(direct.or(tuple));
            } else {
                self.facts.unresolved = true;
            }
        }
    }
}

#[instrument(level = "debug", skip(source, attrs))]
fn cfg_text(source: &str, attrs: &[syn::Attribute]) -> Option<Vec<String>> {
    attrs
        .iter()
        .map(|attr| {
            if !attr.path().is_ident("cfg") {
                return None;
            }
            let (start, end) = span_range(source, attr.span())?;
            Some(source.get(start..end)?.to_string())
        })
        .collect()
}

/// Provenance for a unique `(line, file)` binding in the constructor's shared tree.
#[instrument(level = "debug", skip(source, block, bindings))]
fn tuple_file_origins(
    source: &str,
    block: &syn::Block,
    bindings: &BindingCounts,
) -> BTreeMap<String, TupleFileOrigin> {
    if bindings.opaque_local_names() {
        return BTreeMap::new();
    }
    block.stmts.iter().filter_map(|stmt| {
        let syn::Stmt::Local(local) = stmt else { return None };
        let Pat::Tuple(pat) = &local.pat else { return None };
        let mut elements = pat.elems.iter();
        let (Some(Pat::Ident(line)), Some(Pat::Ident(file)), None) =
            (elements.next(), elements.next(), elements.next()) else { return None };
        if line.ident != "line" || file.ident != "file"
            || line.mutability.is_some() || file.mutability.is_some()
            || line.subpat.is_some() || file.subpat.is_some()
            || bindings.names().get("file") != Some(&1)
            || bindings.uses().get("file") != Some(&1)
        { return None; }
        let cfg = cfg_text(source, &local.attrs)?;
        let Expr::Block(init) = &*local.init.as_ref()?.expr else { return None };
        let Some(syn::Stmt::Expr(Expr::Tuple(tuple), None)) = init.block.stmts.last() else { return None };
        let mut expressions = tuple.elems.iter();
        let (Some(line_expr), Some(file_expr), None) =
            (expressions.next(), expressions.next(), expressions.next()) else { return None };
        let mut inner_bindings = BindingCounts::default();
        inner_bindings.visit_block(&init.block);
        let aliases = caller_aliases(&init.block, &inner_bindings);
        let Expr::MethodCall(line_method) = line_expr else { return None };
        if line_method.method != "line" || !line_method.args.is_empty()
            || !matches!(&*line_method.receiver, Expr::Path(path)
                if path.qself.is_none() && path.path.get_ident().is_some_and(|name| aliases.contains(&name.to_string())))
        { return None; }
        let fact = inspect_initializer(
            source, file_expr, "file", false, FieldKind::CallSiteFile,
            &aliases, &BTreeSet::new(), &BTreeSet::new(), &BTreeMap::new(),
        )?;
        Some(("file".to_string(), TupleFileOrigin { fact, cfg }))
    }).collect()
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
