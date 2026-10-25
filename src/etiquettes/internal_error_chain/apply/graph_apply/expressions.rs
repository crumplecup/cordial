use std::collections::{BTreeMap, BTreeSet};

use syn::Expr;
use syn::spanned::Spanned;
use syn::visit::Visit;
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::super::uses::{macro_has_unhandled_field_use, tokens_construct_type};
use super::super::{Edit, span_range};
use super::types::type_name;
use super::{Change, Shape, VariantChange};

pub(super) struct GraphExprPlanner<'a> {
    file: &'a SharedParsedFile,
    strategy: ErrorFieldStrategy,
    changes: &'a BTreeMap<(String, String), Change>,
    variants: &'a BTreeMap<(String, String), VariantChange>,
    current_impl: Option<String>,
    current_error_trait: bool,
    edits: Vec<Edit>,
    valid: bool,
    constructed: BTreeSet<(String, String)>,
    constructed_variants: BTreeSet<(String, String)>,
    allowed_uses: BTreeSet<(usize, usize)>,
    strict_external: bool,
}

impl<'a> GraphExprPlanner<'a> {
    #[instrument(level = "debug", skip(self))]
    pub(super) fn finish(
        self,
    ) -> (
        Vec<Edit>,
        bool,
        BTreeSet<(String, String)>,
        BTreeSet<(String, String)>,
    ) {
        (
            self.edits,
            self.valid,
            self.constructed,
            self.constructed_variants,
        )
    }
    #[instrument(level = "debug", skip(file, strategy, changes, variants))]
    pub(super) fn new(
        file: &'a SharedParsedFile,
        strategy: ErrorFieldStrategy,
        changes: &'a BTreeMap<(String, String), Change>,
        variants: &'a BTreeMap<(String, String), VariantChange>,
    ) -> Self {
        Self {
            file,
            strategy,
            changes,
            variants,
            current_impl: None,
            current_error_trait: false,
            edits: Vec::new(),
            valid: true,
            constructed: BTreeSet::new(),
            constructed_variants: BTreeSet::new(),
            allowed_uses: BTreeSet::new(),
            strict_external: false,
        }
    }
    #[instrument(level = "debug", skip(self))]
    pub(super) fn strict_external(mut self) -> Self {
        self.strict_external = true;
        self
    }
    #[instrument(level = "trace", skip(self, span))]
    fn source(&self, span: proc_macro2::Span) -> Option<&str> {
        let (start, end) = span_range(self.file.source(), span)?;
        self.file.source().get(start..end)
    }
    #[instrument(level = "debug", skip(self, span))]
    fn edit(&mut self, span: proc_macro2::Span, replacement: String) {
        if let Some((start, end)) = span_range(self.file.source(), span) {
            self.edits.push(Edit {
                start,
                end,
                replacement,
            });
        } else {
            self.valid = false;
        }
    }
    #[instrument(level = "debug", skip(self, span))]
    fn allow(&mut self, span: proc_macro2::Span) {
        if let Some(range) = span_range(self.file.source(), span) {
            self.allowed_uses.insert(range);
        } else {
            self.valid = false;
        }
    }
    #[instrument(level = "debug", skip(self, expr, shape))]
    fn expression(&self, expr: &Expr, shape: Shape) -> Option<String> {
        let text = self.source(expr.span())?;
        match self.strategy {
            ErrorFieldStrategy::Borrowed => match shape {
                Shape::String | Shape::Value => {
                    let Expr::MethodCall(call) = expr else {
                        return None;
                    };
                    if !call.args.is_empty() {
                        return None;
                    }
                    let supported = match shape {
                        Shape::String => matches!(
                            call.method.to_string().as_str(),
                            "clone" | "to_owned" | "to_string"
                        ),
                        Shape::Value => call.method == "clone",
                        Shape::Boxed => false,
                    };
                    if !supported {
                        return None;
                    }
                    self.source(call.receiver.span()).map(str::to_string)
                }
                Shape::Boxed => {
                    let Expr::Call(call) = expr else {
                        return None;
                    };
                    let Expr::Path(path) = &*call.func else {
                        return None;
                    };
                    if path.path.segments.last()?.ident != "new"
                        || path.path.segments.first()?.ident != "Box"
                        || call.args.len() != 1
                    {
                        return None;
                    }
                    self.expression(call.args.first()?, Shape::Value)
                }
            },
            ErrorFieldStrategy::Owned => match shape {
                Shape::String => Some(format!("({text}).to_owned()")),
                Shape::Value => Some(format!("(*({text})).clone()")),
                Shape::Boxed => Some(format!("Box::new((*({text})).clone())")),
            },
        }
    }
}

impl<'ast> Visit<'ast> for GraphExprPlanner<'_> {
    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self
            .current_impl
            .replace(type_name(&item.self_ty).unwrap_or_default());
        let previous_error_trait = self.current_error_trait;
        self.current_error_trait = item.trait_.as_ref().is_some_and(|(path, _)| {
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                == ["std", "error", "Error"]
                || path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    == ["core", "error", "Error"]
        });
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
        self.current_error_trait = previous_error_trait;
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast syn::ExprStruct) {
        let name = item
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let name = match name.as_deref() {
            Some("Self") => self.current_impl.clone(),
            _ => name,
        };
        if let Some(name) = name {
            if item.rest.is_some() && self.changes.keys().any(|key| key.0 == name) {
                self.valid = false;
            }
            for field in &item.fields {
                let syn::Member::Named(member) = &field.member else {
                    continue;
                };
                let key = (name.clone(), member.to_string());
                if let Some(change) = self.changes.get(&key) {
                    self.constructed.insert(key);
                    if let Some(replacement) = self.expression(&field.expr, change.shape) {
                        self.edit(field.expr.span(), replacement);
                    } else {
                        self.valid = false;
                    }
                }
            }
        }
        syn::visit::visit_expr_struct(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_call(&mut self, item: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*item.func {
            let names: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            if let [owner, variant] = names.as_slice() {
                if let Some(change) = self.variants.get(&(owner.clone(), variant.clone())) {
                    self.constructed_variants
                        .insert((owner.clone(), variant.clone()));
                    if item.args.len() != 1 {
                        self.valid = false;
                    } else if let Some(arg) = item.args.first() {
                        if let Some(replacement) = self.expression(arg, change.shape) {
                            self.edit(arg.span(), replacement);
                        } else {
                            self.valid = false;
                        }
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if self.current_error_trait && item.sig.ident == "source" {
            let source_field = self
                .current_impl
                .as_ref()
                .and_then(|name| self.changes.get(&(name.clone(), "source".into())));
            if source_field.is_some() {
                let expression = item.block.stmts.last().and_then(|stmt| match stmt {
                    syn::Stmt::Expr(expr, None) => Some(expr),
                    _ => None,
                });
                let argument = expression.and_then(|expr| match expr {
                    Expr::Call(call)
                        if matches!(&*call.func, Expr::Path(path) if path.path.is_ident("Some"))
                            && call.args.len() == 1 =>
                    {
                        call.args.first()
                    }
                    _ => None,
                });
                match (self.strategy, argument) {
                    (ErrorFieldStrategy::Borrowed, Some(Expr::Reference(reference)))
                        if self_field(&reference.expr, "source") =>
                    {
                        self.allow(reference.expr.span());
                        self.edit(reference.span(), "self.source".into());
                    }
                    (ErrorFieldStrategy::Owned, Some(expr)) if self_field(expr, "source") => {
                        self.allow(expr.span());
                        self.edit(expr.span(), "&self.source".into());
                    }
                    _ => self.valid = false,
                }
            }
        }
        syn::visit::visit_impl_item_fn(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        if item.method == "as_ref"
            && item.args.is_empty()
            && self_field(&item.receiver, "kind")
            && self
                .current_impl
                .as_ref()
                .is_some_and(|name| self.changes.contains_key(&(name.clone(), "kind".into())))
            && self.strategy == ErrorFieldStrategy::Borrowed
        {
            self.allow(item.receiver.span());
            self.edit(item.span(), "self.kind".into());
            return;
        }
        syn::visit::visit_expr_method_call(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_match(&mut self, item: &'ast syn::ExprMatch) {
        if self.strategy == ErrorFieldStrategy::Owned
            && self_field(&item.expr, "kind")
            && self
                .current_impl
                .as_ref()
                .is_some_and(|name| self.changes.contains_key(&(name.clone(), "kind".into())))
        {
            self.allow(item.expr.span());
            self.edit(item.expr.span(), "self.kind.as_ref()".into());
            for arm in &item.arms {
                self.visit_arm(arm);
            }
            return;
        }
        syn::visit::visit_expr_match(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast syn::ExprField) {
        if self.strict_external
            && matches!(&item.member, syn::Member::Named(name)
                if self.changes.keys().any(|(_, field)| field == &name.to_string()))
            && !matches!(&*item.base, Expr::Path(path) if path.path.is_ident("self"))
        {
            self.valid = false;
        }
        if matches!(&*item.base, Expr::Path(path) if path.path.is_ident("self")) {
            if let syn::Member::Named(name) = &item.member {
                let key = (
                    self.current_impl.clone().unwrap_or_default(),
                    name.to_string(),
                );
                if self.changes.contains_key(&key)
                    && span_range(self.file.source(), item.span())
                        .is_none_or(|range| !self.allowed_uses.contains(&range))
                {
                    self.valid = false;
                }
            }
        }
        syn::visit::visit_expr_field(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_pat_struct(&mut self, item: &'ast syn::PatStruct) {
        let name = item
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let name = if name.as_deref() == Some("Self") {
            self.current_impl.clone()
        } else {
            name
        };
        if let Some(name) = name {
            if item.fields.iter().any(|field| {
                matches!(&field.member, syn::Member::Named(member)
                if self.changes.contains_key(&(name.clone(), member.to_string())))
            }) {
                self.valid = false;
            }
        }
        syn::visit::visit_pat_struct(self, item);
    }
    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if self.changes.keys().any(|(name, field)| {
            tokens_construct_type(&item.tokens, name) || macro_has_unhandled_field_use(item, field)
        }) || self
            .variants
            .keys()
            .any(|(name, _)| tokens_mention_ident(&item.tokens, name))
        {
            self.valid = false;
        }
        syn::visit::visit_macro(self, item);
    }
}

#[instrument(level = "debug", skip(expr))]
fn self_field(expr: &Expr, field_name: &str) -> bool {
    matches!(expr, Expr::Field(field)
        if matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self"))
            && matches!(&field.member, syn::Member::Named(name) if name == field_name))
}

#[instrument(level = "debug", skip(tokens))]
fn tokens_mention_ident(tokens: &proc_macro2::TokenStream, name: &str) -> bool {
    tokens.clone().into_iter().any(|tree| match tree {
        proc_macro2::TokenTree::Ident(ident) => ident == name,
        proc_macro2::TokenTree::Group(group) => tokens_mention_ident(&group.stream(), name),
        _ => false,
    })
}
