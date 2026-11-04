//! Resolve the existing CLI dispatch contract over canonical expressions.
use super::super::catalog::{LayoutCatalog, TypeRec, VariantShape};
use super::syntax;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasLoopBody, HasName},
};
use std::collections::{BTreeMap, BTreeSet};
#[tracing::instrument(level = "debug", skip(catalog, body))]
pub(super) fn targets(
    catalog: &LayoutCatalog,
    owner: &str,
    body: &ast::BlockExpr,
) -> BTreeSet<String> {
    let mut resolver = Resolver {
        catalog,
        owner,
        bindings: BTreeMap::new(),
        called: BTreeSet::new(),
    };
    resolver.walk(body.syntax());
    resolver.called
}
struct Resolver<'a> {
    catalog: &'a LayoutCatalog,
    owner: &'a str,
    bindings: BTreeMap<String, Vec<String>>,
    called: BTreeSet<String>,
}
impl Resolver<'_> {
    #[tracing::instrument(level = "trace", skip(self, types))]
    fn primary(&self, types: &[String]) -> Option<String> {
        if types.iter().any(|t| t == self.owner) {
            return Some(self.owner.into());
        }
        types
            .iter()
            .find(|t| self.catalog.types().contains_key(*t))
            .cloned()
    }
    #[tracing::instrument(level = "trace", skip(self, expr))]
    fn expr_types(&self, expr: &ast::Expr) -> Vec<String> {
        match expr {
            ast::Expr::PathExpr(p) => {
                let names = p.path().map(|p| syntax::path_names(&p)).unwrap_or_default();
                if names == ["self"] {
                    vec![self.owner.into()]
                } else if names.len() == 1 {
                    self.bindings.get(&names[0]).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            ast::Expr::FieldExpr(f) => f
                .expr()
                .map(|e| self.expr_types(&e))
                .and_then(|types| self.primary(&types))
                .and_then(|name| self.catalog.types().get(&name))
                .and_then(|rec| f.name_ref().and_then(|name| rec.fields().get(name.text())))
                .cloned()
                .unwrap_or_default(),
            ast::Expr::MethodCallExpr(c)
                if c.name_ref().is_some_and(|n| {
                    matches!(
                        n.text(),
                        "as_ref" | "clone" | "into" | "unwrap" | "expect" | "as_mut" | "copied"
                    )
                }) =>
            {
                c.receiver()
                    .map(|e| self.expr_types(&e))
                    .unwrap_or_default()
            }
            ast::Expr::RefExpr(e) => e.expr().map(|e| self.expr_types(&e)).unwrap_or_default(),
            ast::Expr::ParenExpr(e) => e.expr().map(|e| self.expr_types(&e)).unwrap_or_default(),
            ast::Expr::TryExpr(e) => e.expr().map(|e| self.expr_types(&e)).unwrap_or_default(),
            ast::Expr::PrefixExpr(e) => e.expr().map(|e| self.expr_types(&e)).unwrap_or_default(),
            _ => Vec::new(),
        }
    }
    #[tracing::instrument(level = "trace", skip(self, path))]
    fn pattern_rec(
        &self,
        path: &ast::Path,
        scrutinee: Option<&str>,
    ) -> Option<(&TypeRec, Option<String>)> {
        let names = syntax::path_names(path);
        let first = names.first()?;
        let last = names.last()?;
        let owner = if first == "Self" {
            Some(self.owner)
        } else if self.catalog.types().contains_key(first) {
            Some(first.as_str())
        } else {
            scrutinee
        };
        let rec = self.catalog.types().get(owner?)?;
        if names.len() == 1 && (last == "Self" || last == rec.ident()) {
            return Some((rec, None));
        }
        if rec.variants().contains_key(last) {
            return Some((rec, Some(last.clone())));
        }
        (last == rec.ident()).then_some((rec, None))
    }
    #[tracing::instrument(level = "trace", skip(self, pat, out))]
    fn pattern(&self, pat: &ast::Pat, ty: Option<&str>, out: &mut BTreeMap<String, Vec<String>>) {
        match pat {
            ast::Pat::IdentPat(p) => {
                if let (Some(name), Some(ty)) = (p.name(), ty) {
                    out.insert(name.text().to_string(), vec![ty.into()]);
                }
                if let Some(sub) = p.pat() {
                    self.pattern(&sub, ty, out);
                }
            }
            ast::Pat::RecordPat(p) => {
                let rec = p
                    .path()
                    .as_ref()
                    .and_then(|path| self.pattern_rec(path, ty));
                let fields = rec.and_then(|(rec, variant)| match variant {
                    Some(v) => match rec.variants().get(&v) {
                        Some(VariantShape::Named(fields)) => Some(fields),
                        _ => None,
                    },
                    None => (!rec.fields().is_empty()).then_some(rec.fields()),
                });
                if let Some(list) = p.record_pat_field_list() {
                    for field in list.fields() {
                        let Some(pat) = field.pat() else { continue };
                        let name = field.name_ref().map(|n| n.text().to_string()).or_else(|| {
                            ast::IdentPat::cast(pat.syntax().clone())
                                .and_then(|p| p.name())
                                .map(|n| n.text().to_string())
                        });
                        let types = name
                            .and_then(|name| fields.and_then(|f| f.get(&name)))
                            .cloned()
                            .unwrap_or_default();
                        self.bind_types(&pat, &types, out);
                    }
                }
            }
            ast::Pat::TupleStructPat(p) => {
                let path = p.path();
                if path
                    .as_ref()
                    .and_then(|p| syntax::path_names(p).last().cloned())
                    .is_some_and(|n| matches!(n.as_str(), "Some" | "Ok"))
                {
                    if let Some(pat) = p.fields().next() {
                        self.pattern(&pat, ty, out);
                    }
                    return;
                }
                let rec = path.as_ref().and_then(|path| self.pattern_rec(path, ty));
                let fields = rec
                    .and_then(|(rec, v)| v.and_then(|v| rec.variants().get(&v)))
                    .and_then(|v| match v {
                        VariantShape::Unnamed(f) => Some(f),
                        _ => None,
                    });
                for (i, pat) in p.fields().enumerate() {
                    let types = fields.and_then(|f| f.get(i)).cloned().unwrap_or_default();
                    self.bind_types(&pat, &types, out);
                }
            }
            ast::Pat::TuplePat(p) => {
                for p in p.fields() {
                    self.pattern(&p, ty, out);
                }
            }
            ast::Pat::RefPat(p) => {
                if let Some(p) = p.pat() {
                    self.pattern(&p, ty, out);
                }
            }
            ast::Pat::OrPat(p) => {
                for p in p.pats() {
                    self.pattern(&p, ty, out);
                }
            }
            ast::Pat::ParenPat(p) => {
                if let Some(p) = p.pat() {
                    self.pattern(&p, ty, out);
                }
            }
            _ => {}
        }
    }
    #[tracing::instrument(level = "trace", skip(self, pat, types, out))]
    fn bind_types(
        &self,
        pat: &ast::Pat,
        types: &[String],
        out: &mut BTreeMap<String, Vec<String>>,
    ) {
        match pat {
            ast::Pat::IdentPat(p) => {
                if let Some(name) = p.name() {
                    out.insert(name.text().to_string(), types.to_vec());
                }
                if let Some(p) = p.pat() {
                    self.bind_types(&p, types, out);
                }
            }
            ast::Pat::RefPat(p) => {
                if let Some(p) = p.pat() {
                    self.bind_types(&p, types, out);
                }
            }
            _ => {
                let ty = types.iter().find(|n| self.catalog.types().contains_key(*n));
                self.pattern(pat, ty.map(String::as_str), out);
            }
        }
    }
    #[tracing::instrument(level = "trace", skip(self, pat, expr, body))]
    fn scoped(&mut self, pat: &ast::Pat, expr: &ast::Expr, body: &SyntaxNode) {
        let ty = self.primary(&self.expr_types(expr));
        let mut extra = BTreeMap::new();
        self.pattern(pat, ty.as_deref(), &mut extra);
        let old = self.bindings.clone();
        self.bindings.extend(extra);
        self.walk(body);
        self.bindings = old;
    }
    #[tracing::instrument(level = "trace", skip(self, node))]
    fn walk(&mut self, node: &SyntaxNode) {
        if ast::TokenTree::can_cast(node.kind()) {
            return;
        }
        if let Some(expr) = ast::Expr::cast(node.clone()) {
            match expr {
                ast::Expr::MethodCallExpr(call) => {
                    if call.name_ref().is_some_and(|n| n.text() == "act")
                        && let Some(receiver) = call.receiver()
                    {
                        self.called.extend(self.expr_types(&receiver));
                    }
                }
                ast::Expr::CallExpr(call) => {
                    if let Some(ast::Expr::PathExpr(path)) = call.expr()
                        && let Some(path) = path.path()
                    {
                        let names = syntax::path_names(&path);
                        if names.last().is_some_and(|n| n == "act") {
                            if names.len() >= 2 {
                                let name = &names[names.len() - 2];
                                self.called.insert(if name == "Self" {
                                    self.owner.into()
                                } else {
                                    name.clone()
                                });
                            }
                            if let Some(arg) = call.arg_list().and_then(|a| a.args().next()) {
                                self.called.extend(self.expr_types(&arg));
                            }
                        }
                    }
                }
                ast::Expr::MatchExpr(expr) => {
                    if let Some(scrutinee) = expr.expr() {
                        self.walk(scrutinee.syntax());
                        if let Some(list) = expr.match_arm_list() {
                            for arm in list.arms() {
                                if let (Some(pat), Some(body)) = (arm.pat(), arm.expr()) {
                                    self.scoped(&pat, &scrutinee, body.syntax());
                                }
                            }
                        }
                    }
                    return;
                }
                ast::Expr::IfExpr(expr) => {
                    if let Some(ast::Expr::LetExpr(cond)) = expr.condition()
                        && let (Some(pat), Some(scrutinee)) = (cond.pat(), cond.expr())
                    {
                        self.walk(scrutinee.syntax());
                        if let Some(body) = expr.then_branch() {
                            self.scoped(&pat, &scrutinee, body.syntax());
                        }
                        if let Some(branch) = expr.else_branch() {
                            match branch {
                                ast::ElseBranch::Block(b) => self.walk(b.syntax()),
                                ast::ElseBranch::IfExpr(b) => self.walk(b.syntax()),
                            }
                        }
                        return;
                    }
                }
                ast::Expr::WhileExpr(expr) => {
                    if let Some(ast::Expr::LetExpr(cond)) = expr.condition()
                        && let (Some(pat), Some(scrutinee)) = (cond.pat(), cond.expr())
                    {
                        self.walk(scrutinee.syntax());
                        if let Some(body) = expr.loop_body() {
                            self.scoped(&pat, &scrutinee, body.syntax());
                        }
                        return;
                    }
                }
                _ => {}
            }
        }
        if let Some(local) = ast::LetStmt::cast(node.clone()) {
            if let Some(expr) = local.initializer() {
                self.walk(expr.syntax());
                let ty = self.primary(&self.expr_types(&expr));
                let mut extra = BTreeMap::new();
                if let Some(pat) = local.pat() {
                    self.pattern(&pat, ty.as_deref(), &mut extra);
                }
                self.bindings.extend(extra);
            }
            return;
        }
        for child in node.children() {
            self.walk(&child);
        }
    }
}
