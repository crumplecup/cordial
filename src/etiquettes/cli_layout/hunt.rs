//! Nested clap `act` call hunting and syn type-ident helpers.

use std::collections::{BTreeMap, BTreeSet};

use syn::visit::Visit;
use syn::{Member, Pat};

use super::scan::catalog::{ActRec, LayoutCatalog, TypeRec, VariantShape};

use tracing::instrument;
#[instrument(level = "debug", skip(catalog))]
pub(super) fn finalize_acts(catalog: &mut LayoutCatalog) {
    let pending = std::mem::take(&mut catalog.pending_acts);
    for act in pending {
        let called_on = resolve_act_targets(catalog, &act.ident, &act.block);
        catalog.acts.insert(
            act.ident,
            ActRec {
                file: act.file,
                line: act.line,
                called_on,
            },
        );
    }
}

#[instrument(level = "debug", skip(item, clap_idents))]
pub(super) fn nested_clap_types(
    item: &TypeRec,
    clap_idents: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut consider = |idents: &[String]| {
        for ident in idents {
            if clap_idents.contains(ident) && ident != &item.ident {
                out.insert(ident.clone());
            }
        }
    };
    for idents in item.fields.values() {
        consider(idents);
    }
    for variant in item.variants.values() {
        match variant {
            VariantShape::Named(fields) => {
                for idents in fields.values() {
                    consider(idents);
                }
            }
            VariantShape::Unnamed(fields) => {
                for idents in fields {
                    consider(idents);
                }
            }
            VariantShape::Unit => {}
        }
    }
    out
}

struct ActCallHunt<'a> {
    catalog: &'a LayoutCatalog,
    self_ident: &'a str,
    bindings: BTreeMap<String, Vec<String>>,
    called_on: BTreeSet<String>,
}

#[instrument(level = "debug", skip(catalog, block))]
fn resolve_act_targets(
    catalog: &LayoutCatalog,
    self_ident: &str,
    block: &syn::Block,
) -> BTreeSet<String> {
    let mut hunt = ActCallHunt {
        catalog,
        self_ident,
        bindings: BTreeMap::new(),
        called_on: BTreeSet::new(),
    };
    hunt.visit_block(block);
    hunt.called_on
}

impl ActCallHunt<'_> {
    #[instrument(level = "debug", skip(self))]
    fn primary_type(&self, idents: &[String]) -> Option<String> {
        if idents.iter().any(|ident| ident == self.self_ident) {
            return Some(self.self_ident.to_string());
        }
        idents
            .iter()
            .find(|ident| self.catalog.types.contains_key(*ident))
            .cloned()
    }

    #[instrument(level = "debug", skip(self, expr))]
    fn expr_type_idents(&self, expr: &syn::Expr) -> Vec<String> {
        match expr {
            syn::Expr::Path(path) if path.path.is_ident("self") => {
                vec![self.self_ident.to_string()]
            }
            syn::Expr::Path(path) if path.path.segments.len() == 1 => {
                let name = path.path.segments[0].ident.to_string();
                self.bindings.get(&name).cloned().unwrap_or_default()
            }
            syn::Expr::Field(field) => {
                let base = self.expr_type_idents(&field.base);
                let Some(type_name) = self.primary_type(&base) else {
                    return Vec::new();
                };
                let member = match &field.member {
                    Member::Named(ident) => ident.to_string(),
                    Member::Unnamed(index) => index.index.to_string(),
                };
                self.catalog
                    .types
                    .get(&type_name)
                    .and_then(|rec| rec.fields.get(&member))
                    .cloned()
                    .unwrap_or_default()
            }
            syn::Expr::MethodCall(call)
                if call.method == "as_ref"
                    || call.method == "clone"
                    || call.method == "into"
                    || call.method == "unwrap"
                    || call.method == "expect"
                    || call.method == "as_mut"
                    || call.method == "copied" =>
            {
                self.expr_type_idents(&call.receiver)
            }
            syn::Expr::Reference(reference) => self.expr_type_idents(&reference.expr),
            syn::Expr::Paren(paren) => self.expr_type_idents(&paren.expr),
            syn::Expr::Group(group) => self.expr_type_idents(&group.expr),
            syn::Expr::Try(expr) => self.expr_type_idents(&expr.expr),
            syn::Expr::Unary(unary) => self.expr_type_idents(&unary.expr),
            _ => Vec::new(),
        }
    }

    #[instrument(level = "debug", skip(self, path))]
    fn pattern_rec<'a>(
        &'a self,
        path: &syn::Path,
        scrutinee: Option<&str>,
    ) -> Option<(&'a TypeRec, Option<String>)> {
        let last = path.segments.last()?.ident.to_string();
        let first = path.segments.first()?.ident.to_string();
        let owner = if first == "Self" {
            Some(self.self_ident.to_string())
        } else if self.catalog.types.contains_key(&first) {
            Some(first)
        } else {
            scrutinee.map(str::to_string)
        };
        let rec = owner
            .as_deref()
            .and_then(|name| self.catalog.types.get(name))?;
        if path.segments.len() == 1 && (last == "Self" || last == rec.ident) {
            return Some((rec, None));
        }
        if rec.variants.contains_key(&last) {
            return Some((rec, Some(last)));
        }
        if last == rec.ident {
            return Some((rec, None));
        }
        None
    }

    #[instrument(level = "debug", skip(rec))]
    fn named_fields_of<'a>(
        rec: &'a TypeRec,
        variant: Option<&str>,
    ) -> Option<&'a BTreeMap<String, Vec<String>>> {
        if let Some(variant) = variant {
            match rec.variants.get(variant)? {
                VariantShape::Named(fields) => Some(fields),
                _ => None,
            }
        } else if rec.fields.is_empty() {
            None
        } else {
            Some(&rec.fields)
        }
    }

    #[instrument(level = "debug", skip(rec))]
    fn unnamed_fields_of<'a>(
        rec: &'a TypeRec,
        variant: Option<&str>,
    ) -> Option<&'a Vec<Vec<String>>> {
        let variant = variant?;
        match rec.variants.get(variant)? {
            VariantShape::Unnamed(fields) => Some(fields),
            _ => None,
        }
    }

    #[instrument(level = "debug", skip(self, pat, out))]
    fn collect_pat_bindings(
        &self,
        pat: &Pat,
        type_name: Option<&str>,
        out: &mut BTreeMap<String, Vec<String>>,
    ) {
        match pat {
            Pat::Ident(ident) => {
                if let Some(ty) = type_name {
                    out.insert(ident.ident.to_string(), vec![ty.to_string()]);
                }
                if let Some((_, sub)) = &ident.subpat {
                    self.collect_pat_bindings(sub, type_name, out);
                }
            }
            Pat::Struct(strct) => {
                let rec = self.pattern_rec(&strct.path, type_name);
                let fields =
                    rec.and_then(|(rec, variant)| Self::named_fields_of(rec, variant.as_deref()));
                for field in &strct.fields {
                    let member = match &field.member {
                        Member::Named(ident) => ident.to_string(),
                        Member::Unnamed(index) => index.index.to_string(),
                    };
                    let field_tys = fields
                        .and_then(|map| map.get(&member))
                        .cloned()
                        .unwrap_or_default();
                    self.bind_pat_to_types(&field.pat, &field_tys, out);
                }
            }
            Pat::TupleStruct(tuple) => {
                let rec = self.pattern_rec(&tuple.path, type_name);
                let fields =
                    rec.and_then(|(rec, variant)| Self::unnamed_fields_of(rec, variant.as_deref()));
                for (index, elem) in tuple.elems.iter().enumerate() {
                    let field_tys = fields
                        .and_then(|list| list.get(index))
                        .cloned()
                        .unwrap_or_default();
                    self.bind_pat_to_types(elem, &field_tys, out);
                }
            }
            Pat::Tuple(tuple) => {
                for elem in &tuple.elems {
                    self.collect_pat_bindings(elem, type_name, out);
                }
            }
            Pat::Reference(reference) => {
                self.collect_pat_bindings(&reference.pat, type_name, out);
            }
            Pat::Or(or_pat) => {
                for case in &or_pat.cases {
                    self.collect_pat_bindings(case, type_name, out);
                }
            }
            Pat::Paren(paren) => self.collect_pat_bindings(&paren.pat, type_name, out),
            Pat::Type(typed) => self.collect_pat_bindings(&typed.pat, type_name, out),
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, pat, out))]
    fn bind_pat_to_types(
        &self,
        pat: &Pat,
        field_tys: &[String],
        out: &mut BTreeMap<String, Vec<String>>,
    ) {
        match pat {
            Pat::Ident(ident) => {
                out.insert(ident.ident.to_string(), field_tys.to_vec());
                if let Some((_, sub)) = &ident.subpat {
                    self.bind_pat_to_types(sub, field_tys, out);
                }
            }
            Pat::Reference(reference) => {
                self.bind_pat_to_types(&reference.pat, field_tys, out);
            }
            other => {
                let inner = field_tys
                    .iter()
                    .find(|ident| self.catalog.types.contains_key(*ident))
                    .map(String::as_str);
                self.collect_pat_bindings(other, inner, out);
            }
        }
    }

    #[instrument(level = "trace", skip(self, pat, scrutinee, visit))]
    fn with_pat_bindings(
        &mut self,
        pat: &Pat,
        scrutinee: &syn::Expr,
        visit: impl FnOnce(&mut Self),
    ) {
        let scrutinee_types = self.expr_type_idents(scrutinee);
        let type_name = self.primary_type(&scrutinee_types);
        let mut extra = BTreeMap::new();
        self.collect_pat_bindings(pat, type_name.as_deref(), &mut extra);
        let old = self.bindings.clone();
        for (name, idents) in extra {
            self.bindings.insert(name, idents);
        }
        visit(self);
        self.bindings = old;
    }
}

impl<'ast> Visit<'ast> for ActCallHunt<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "act" {
            for ident in self.expr_type_idents(&node.receiver) {
                self.called_on.insert(ident);
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref() {
            let segments = &path.path.segments;
            if segments
                .last()
                .is_some_and(|segment| segment.ident == "act")
            {
                if segments.len() >= 2 {
                    let ty = segments[segments.len() - 2].ident.to_string();
                    if ty == "Self" {
                        self.called_on.insert(self.self_ident.to_string());
                    } else {
                        self.called_on.insert(ty);
                    }
                }
                if let Some(first) = node.args.first() {
                    for ident in self.expr_type_idents(first) {
                        self.called_on.insert(ident);
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        syn::visit::visit_expr(self, &node.expr);
        for arm in &node.arms {
            self.with_pat_bindings(&arm.pat, &node.expr, |hunt| {
                if let Some((_, guard)) = &arm.guard {
                    syn::visit::visit_expr(hunt, guard);
                }
                syn::visit::visit_expr(hunt, &arm.body);
            });
        }
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let syn::Expr::Let(expr_let) = node.cond.as_ref() {
            syn::visit::visit_expr(self, &expr_let.expr);
            self.with_pat_bindings(&expr_let.pat, &expr_let.expr, |hunt| {
                syn::visit::visit_block(hunt, &node.then_branch);
            });
            if let Some((_, else_branch)) = &node.else_branch {
                syn::visit::visit_expr(self, else_branch);
            }
            return;
        }
        syn::visit::visit_expr_if(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        if let syn::Expr::Let(expr_let) = node.cond.as_ref() {
            syn::visit::visit_expr(self, &expr_let.expr);
            self.with_pat_bindings(&expr_let.pat, &expr_let.expr, |hunt| {
                syn::visit::visit_block(hunt, &node.body);
            });
            return;
        }
        syn::visit::visit_expr_while(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let Some(init) = &node.init {
            syn::visit::visit_expr(self, &init.expr);
            let scrutinee_types = self.expr_type_idents(&init.expr);
            let type_name = self.primary_type(&scrutinee_types);
            let mut extra = BTreeMap::new();
            self.collect_pat_bindings(&node.pat, type_name.as_deref(), &mut extra);
            for (name, idents) in extra {
                self.bindings.insert(name, idents);
            }
        }
    }
}
