//! Unambiguous immutable bindings and caller aliases for one constructor body.
use super::native_values::{bare, location_caller, range};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasAttrs, HasName},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

#[derive(Default, derive_getters::Getters)]
pub(super) struct Counts {
    names: BTreeMap<String, usize>,
    uses: BTreeMap<String, usize>,
    #[getter(copy)]
    opaque: bool,
}

impl Counts {
    #[instrument(level = "trace", skip(block))]
    pub(super) fn collect(block: &ast::BlockExpr) -> Self {
        let mut counts = Self::default();
        counts.visit(block.syntax());
        counts
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit(&mut self, node: &SyntaxNode) {
        if matches!(node.kind(), SyntaxKind::TOKEN_TREE | SyntaxKind::ATTR) {
            return;
        }
        let name = if let Some(pat) = ast::IdentPat::cast(node.clone()) {
            pat.name()
        } else {
            match ast::Item::cast(node.clone()).filter(|_| {
                node.parent()
                    .is_none_or(|parent| parent.kind() != SyntaxKind::ASSOC_ITEM_LIST)
            }) {
                Some(ast::Item::Const(item)) => item.name(),
                Some(ast::Item::Static(item)) => item.name(),
                Some(ast::Item::Fn(item)) => item.name(),
                Some(ast::Item::Struct(item)) => item.name(),
                Some(ast::Item::Enum(item)) => item.name(),
                Some(ast::Item::Union(item)) => item.name(),
                Some(ast::Item::Use(_) | ast::Item::MacroRules(_) | ast::Item::MacroDef(_)) => {
                    self.opaque = true;
                    None
                }
                Some(ast::Item::MacroCall(_))
                    if node.parent().is_some_and(|parent| {
                        matches!(
                            parent.kind(),
                            SyntaxKind::ITEM_LIST | SyntaxKind::SOURCE_FILE
                        )
                    }) =>
                {
                    self.opaque = true;
                    None
                }
                _ => None,
            }
        };
        if let Some(name) = name {
            *self.names.entry(name.text().to_string()).or_default() += 1;
        }
        if let Some(path) = ast::PathExpr::cast(node.clone())
            .and_then(|expr| expr.path())
            .and_then(|path| bare(&path))
        {
            *self.uses.entry(path).or_default() += 1;
        }
        // Also count shorthand when the parser exposes a direct name reference.
        if let Some(field) =
            ast::RecordExprField::cast(node.clone()).filter(|field| field.expr().is_none())
            && let Some(name) = field.name_ref()
        {
            *self.uses.entry(name.text().to_string()).or_default() += 1;
        }
        for child in node.children() {
            self.visit(&child);
        }
    }
}

#[instrument(level = "trace", skip(block, counts))]
pub(super) fn aliases(block: &ast::BlockExpr, counts: &Counts) -> BTreeSet<String> {
    if counts.opaque {
        return BTreeSet::new();
    }
    block
        .stmt_list()
        .into_iter()
        .flat_map(|list| list.statements())
        .filter_map(|stmt| {
            let ast::Stmt::LetStmt(local) = stmt else {
                return None;
            };
            if local.ty().is_some() {
                return None;
            }
            let ast::Pat::IdentPat(pat) = local.pat()? else {
                return None;
            };
            let name = pat.name()?.text().to_string();
            if pat.mut_token().is_some() || counts.names.get(&name) != Some(&1) {
                return None;
            }
            let ast::Expr::CallExpr(call) = local.initializer()? else {
                return None;
            };
            location_caller(&call).then_some(name)
        })
        .collect()
}

#[instrument(level = "trace", skip(ty))]
fn string_lifetime(ty: ast::Type) -> Option<String> {
    let ast::Type::RefType(reference) = ty else {
        return None;
    };
    if reference.mut_token().is_some() {
        return None;
    }
    let ast::Type::PathType(ty) = reference.ty()? else {
        return None;
    };
    if ty.path().and_then(|path| bare(&path)).as_deref() != Some("str") {
        return None;
    }
    Some(
        reference
            .lifetime()?
            .syntax()
            .text()
            .to_string()
            .trim_start_matches('\'')
            .to_string(),
    )
}

#[instrument(level = "trace", skip(item))]
fn no_attrs(item: &impl HasAttrs) -> bool {
    item.attrs().next().is_none() && item.doc_comments().next().is_none()
}

#[instrument(level = "trace", skip(function, block, counts))]
pub(super) fn strings(
    function: &ast::Fn,
    block: &ast::BlockExpr,
    counts: &Counts,
) -> BTreeMap<String, (String, usize)> {
    let mut bindings = BTreeMap::new();
    if counts.opaque {
        return bindings;
    }
    let mut arguments = BTreeSet::new();
    for param in function
        .param_list()
        .into_iter()
        .flat_map(|list| list.params())
    {
        let Some(ast::Pat::IdentPat(pat)) = param.pat() else {
            continue;
        };
        let Some(name) = pat.name().map(|name| name.text().to_string()) else {
            continue;
        };
        arguments.insert(name.clone());
        if pat.pat().is_none()
            && !counts.names.contains_key(&name)
            && let Some(lifetime) = param.ty().and_then(string_lifetime)
        {
            bindings.insert(name, (lifetime, 0));
        }
    }
    for stmt in block
        .stmt_list()
        .into_iter()
        .flat_map(|list| list.statements())
    {
        let found = match stmt {
            ast::Stmt::LetStmt(local) if no_attrs(&local) && local.initializer().is_some() => {
                match local.pat() {
                    Some(ast::Pat::IdentPat(pat))
                        if pat.mut_token().is_none() && pat.pat().is_none() =>
                    {
                        pat.name().zip(local.ty().and_then(string_lifetime)).map(
                            |(name, lifetime)| {
                                (name.text().to_string(), lifetime, range(local.syntax()).1)
                            },
                        )
                    }
                    _ => None,
                }
            }
            ast::Stmt::Item(ast::Item::Const(item)) if no_attrs(&item) => item
                .name()
                .zip(
                    item.ty()
                        .and_then(string_lifetime)
                        .filter(|lifetime| lifetime == "static"),
                )
                .map(|(name, lifetime)| (name.text().to_string(), lifetime, 0)),
            ast::Stmt::Item(ast::Item::Static(item))
                if no_attrs(&item) && item.mut_token().is_none() =>
            {
                item.name()
                    .zip(
                        item.ty()
                            .and_then(string_lifetime)
                            .filter(|lifetime| lifetime == "static"),
                    )
                    .map(|(name, lifetime)| (name.text().to_string(), lifetime, 0))
            }
            _ => None,
        };
        if let Some((name, lifetime, end)) = found
            && !arguments.contains(&name)
            && counts.names.get(&name) == Some(&1)
        {
            bindings.insert(name, (lifetime, end));
        }
    }
    bindings
}
