//! Resolve filename receivers while walking shared syntax and macro islands.

use std::collections::BTreeMap;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, FnArg, Pat, ReturnType};
use tracing::instrument;

use crate::loader::{SharedMacroSyntax, SharedParsedFile};

use super::{GetterUseFact, Identities, Receiver, UseKind};

#[derive(Clone)]
struct Scope {
    bindings: BTreeMap<String, Receiver>,
    functions: BTreeMap<String, Receiver>,
    owner: Receiver,
    local_types: bool,
}

struct Collector<'a> {
    file: &'a SharedParsedFile,
    identities: &'a Identities,
    scope: Scope,
    facts: Vec<GetterUseFact>,
}

#[instrument(level = "debug", skip(file, identities))]
pub(super) fn collect(file: &SharedParsedFile, identities: &Identities) -> Vec<GetterUseFact> {
    let mut collector = Collector {
        file,
        identities,
        facts: Vec::new(),
        scope: Scope {
            bindings: BTreeMap::new(),
            functions: BTreeMap::new(),
            owner: Receiver::Unknown,
            local_types: false,
        },
    };
    collector.visit_file(file.syntax());
    collector.facts
}

impl Collector<'_> {
    #[instrument(level = "debug", skip(self, span, receiver, kind))]
    fn record(&mut self, span: proc_macro2::Span, receiver: Receiver, kind: UseKind) {
        self.facts.push(GetterUseFact {
            path: self.file.path().to_path_buf(),
            line: span.start().line,
            column: span.start().column,
            receiver,
            kind,
            end_line: span.end().line,
            end_column: span.end().column,
        });
    }

    #[instrument(level = "trace", skip(self, ty))]
    fn ty(&self, ty: &syn::Type) -> Receiver {
        if self.scope.local_types {
            return Receiver::Unknown;
        }
        if matches!(ty, syn::Type::Path(path) if path.path.is_ident("Self")) {
            self.scope.owner.clone()
        } else {
            self.identities.ty(self.file, ty)
        }
    }

    #[instrument(level = "trace", skip(self, expr))]
    fn receiver(&self, expr: &Expr) -> Receiver {
        match expr {
            Expr::Path(path) if path.qself.is_none() => path
                .path
                .get_ident()
                .and_then(|name| self.scope.bindings.get(&name.to_string()))
                .cloned()
                .unwrap_or(Receiver::Unknown),
            Expr::Reference(reference) => self.receiver(&reference.expr),
            Expr::Paren(paren) => self.receiver(&paren.expr),
            Expr::Call(call) => {
                let Expr::Path(path) = &*call.func else {
                    return Receiver::Unknown;
                };
                if path.qself.is_some() {
                    return Receiver::Unknown;
                }
                if let Some(name) = path.path.get_ident() {
                    if self.scope.bindings.contains_key(&name.to_string()) {
                        return Receiver::Unknown;
                    }
                    return self
                        .scope
                        .functions
                        .get(&name.to_string())
                        .cloned()
                        .unwrap_or(Receiver::Unknown);
                }
                let mut owner = path.path.clone();
                let method = owner.segments.pop();
                if method.is_some_and(|part| part.ident == "caller")
                    && call.args.is_empty()
                    && !self.scope.local_types
                    && self.identities.path(self.file, &owner) == Receiver::Location
                {
                    Receiver::Location
                } else {
                    Receiver::Unknown
                }
            }
            _ => Receiver::Unknown,
        }
    }

    #[instrument(level = "trace", skip(self, signature, body))]
    fn function(&mut self, signature: &syn::Signature, body: &syn::Block) {
        let previous = self.scope.clone();
        self.scope.bindings.clear();
        self.scope.functions.clear();
        self.scope.local_types |= signature.generics.type_params().next().is_some();
        for input in &signature.inputs {
            match input {
                FnArg::Receiver(_) => {
                    self.scope
                        .bindings
                        .insert("self".into(), self.scope.owner.clone());
                }
                FnArg::Typed(input) => {
                    let receiver = self.ty(&input.ty);
                    bind(&mut self.scope.bindings, &input.pat, receiver);
                }
            }
        }
        self.visit_block(body);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, tokens))]
    fn opaque(&mut self, tokens: proc_macro2::TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                proc_macro2::TokenTree::Group(group) => self.opaque(group.stream()),
                proc_macro2::TokenTree::Ident(name) if name == "file" && index > 0 => {
                    if matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(punct) if matches!(punct.as_char(), '.' | ':'))
                    {
                        self.record(name.span(), Receiver::Unknown, UseKind::Getter);
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let previous = self.scope.clone();
        self.scope.local_types |= item.content.is_some();
        syn::visit::visit_item_mod(self, item);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.scope.clone();
        self.scope.owner = self.ty(&item.self_ty);
        self.scope.local_types |= item.generics.type_params().next().is_some();
        syn::visit::visit_item_impl(self, item);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let owner = std::mem::replace(&mut self.scope.owner, Receiver::Unknown);
        self.function(&item.sig, &item.block);
        self.scope.owner = owner;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.function(&item.sig, &item.block);
    }

    #[instrument(level = "trace", skip(self, block))]
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let previous = self.scope.clone();
        self.scope.local_types |= block.stmts.iter().any(|stmt| {
            matches!(
                stmt,
                syn::Stmt::Item(
                    syn::Item::Use(_)
                        | syn::Item::Type(_)
                        | syn::Item::Struct(_)
                        | syn::Item::Enum(_)
                        | syn::Item::Mod(_)
                )
            ) || matches!(stmt, syn::Stmt::Item(syn::Item::Macro(item)) if item.ident.is_some())
        });
        for stmt in &block.stmts {
            if let syn::Stmt::Item(syn::Item::Fn(item)) = stmt {
                let output = match &item.sig.output {
                    ReturnType::Type(_, ty) if item.sig.generics.type_params().next().is_none() => {
                        self.ty(ty)
                    }
                    _ => Receiver::Unknown,
                };
                let key = item.sig.ident.to_string();
                self.scope
                    .functions
                    .entry(key)
                    .and_modify(|receiver| *receiver = Receiver::Unknown)
                    .or_insert(output);
            }
        }
        syn::visit::visit_block(self, block);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, local))]
    fn visit_local(&mut self, local: &'ast syn::Local) {
        let receiver = match (&local.pat, &local.init) {
            (Pat::Type(typed), _) => self.ty(&typed.ty),
            (_, Some(init)) => self.receiver(&init.expr),
            _ => Receiver::Unknown,
        };
        syn::visit::visit_local(self, local);
        bind(&mut self.scope.bindings, &local.pat, receiver);
    }

    #[instrument(level = "trace", skip(self, pat))]
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        self.scope
            .bindings
            .insert(pat.ident.to_string(), Receiver::Unknown);
        syn::visit::visit_pat_ident(self, pat);
    }

    #[instrument(level = "trace", skip(self, closure))]
    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let previous = self.scope.clone();
        for input in &closure.inputs {
            bind(&mut self.scope.bindings, input, Receiver::Unknown);
        }
        syn::visit::visit_expr_closure(self, closure);
        self.scope = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast syn::ExprField) {
        if matches!(&item.member, syn::Member::Named(name) if name == "file") {
            self.record(item.span(), self.receiver(&item.base), UseKind::Field);
        }
        syn::visit::visit_expr_field(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        if item.method == "file" {
            let receiver = self
                .identities
                .method_owner(self.receiver(&item.receiver), false);
            self.record(item.span(), receiver, UseKind::Getter);
        }
        syn::visit::visit_expr_method_call(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        if item.path.segments.len() > 1
            && item
                .path
                .segments
                .last()
                .is_some_and(|part| part.ident == "file")
        {
            let mut path = item.path.clone();
            path.segments.pop();
            let receiver = if self.scope.local_types || item.qself.is_some() {
                Receiver::Unknown
            } else if path.is_ident("Self") {
                self.scope.owner.clone()
            } else {
                self.identities.path(self.file, &path)
            };
            self.record(
                item.span(),
                self.identities.method_owner(receiver, true),
                UseKind::Getter,
            );
        }
        syn::visit::visit_expr_path(self, item);
    }

    #[instrument(level = "trace", skip(self, invocation))]
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if !self.scope.local_types && macro_contract(self.file, invocation) {
            match self.file.macro_syntax(invocation) {
                Some(SharedMacroSyntax::Expressions(expressions)) => {
                    self.record(invocation.span(), Receiver::Unknown, UseKind::Macro);
                    for expression in expressions {
                        self.visit_expr(expression);
                    }
                    return;
                }
                Some(SharedMacroSyntax::Items(items)) => {
                    self.record(invocation.span(), Receiver::Unknown, UseKind::Macro);
                    for item in items {
                        self.visit_item(item);
                    }
                    return;
                }
                None => {}
            }
        }
        self.opaque(invocation.tokens.clone());
    }
}

#[instrument(level = "trace", skip(bindings, pat, receiver))]
fn bind(bindings: &mut BTreeMap<String, Receiver>, pat: &Pat, receiver: Receiver) {
    match pat {
        Pat::Ident(binding) => {
            let value = if binding.mutability.is_none()
                && binding.by_ref.is_none()
                && binding.subpat.is_none()
            {
                receiver
            } else {
                Receiver::Unknown
            };
            bindings.insert(binding.ident.to_string(), value);
        }
        Pat::Type(typed) => bind(bindings, &typed.pat, receiver),
        Pat::Tuple(tuple) => {
            for pat in &tuple.elems {
                bind(bindings, pat, Receiver::Unknown);
            }
        }
        _ => {}
    }
}

#[instrument(level = "trace", skip(file, invocation))]
fn macro_contract(file: &SharedParsedFile, invocation: &syn::Macro) -> bool {
    let Some(name) = invocation
        .path
        .segments
        .last()
        .map(|part| part.ident.to_string())
    else {
        return false;
    };
    if file.syntax().items.iter().any(|item| matches!(item, syn::Item::Macro(item) if item.ident.as_ref().is_some_and(|ident| ident == &name))) { return false; }
    if matches!(name.as_str(), "harness" | "gallery_harness") {
        return super::super::getters::known_derive(
            file.syntax(),
            &invocation.path,
            "amenable_derive",
            &name,
        );
    }
    (invocation.path.segments.len() == 1 && super::identity::unshadowed(file, &name))
        || super::super::getters::known_derive(file.syntax(), &invocation.path, "std", &name)
        || super::super::getters::known_derive(file.syntax(), &invocation.path, "core", &name)
}
