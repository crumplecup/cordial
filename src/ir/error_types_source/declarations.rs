//! Wrapper relationships and error implementors from canonical declarations.

use super::Fact;
use crate::etiquettes::internal_error_chain::InternalErrorTypeProbeId;
use crate::ir::error_chains_source::types::{is_string, type_label};
use crate::ir::error_sites_source::native::path_label;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};
use std::collections::BTreeSet;

#[tracing::instrument(level = "trace", skip(item))]
pub(super) fn wrapper(item: &ast::Struct, prefix: &[String]) -> Vec<Fact> {
    let Some(name) = item.name() else {
        return Vec::new();
    };
    let name = name.text().to_string();
    let mut path = prefix.to_vec();
    let mut modules: Vec<_> = item
        .syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .filter_map(|module| module.name())
        .map(|name| name.text().to_string())
        .collect();
    modules.reverse();
    path.extend(modules);
    path.push(name.clone());
    let path = path.join("::");
    if name == "CordialError" {
        return vec![Fact {
            path,
            probe: InternalErrorTypeProbeId::InternalLink001,
            target: Some("CordialErrorKind".into()),
            node: item.syntax().clone(),
            snippet: format!("struct {name} {{ kind: CordialErrorKind, … }}"),
        }];
    }
    let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() else {
        return Vec::new();
    };
    fields
        .fields()
        .filter(|field| field.name().is_some_and(|name| name.text() == "source"))
        .filter_map(|field| {
            let ty = field.ty()?;
            (!is_string(&ty)).then(|| Fact {
                path: path.clone(),
                probe: InternalErrorTypeProbeId::InternalLink001,
                target: Some(type_label(&ty)),
                node: field.syntax().clone(),
                snippet: format!("struct {name} {{ source: {} }}", type_label(&ty)),
            })
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(item, implementors))]
pub(super) fn source_impl(item: &ast::Impl, implementors: &mut BTreeSet<String>) -> Option<Fact> {
    let ast::Type::PathType(trait_ty) = item.trait_()? else {
        return None;
    };
    if trait_ty.path()?.segment()?.name_ref()?.text() != "Error" {
        return None;
    }
    let name = type_label(&item.self_ty()?);
    implementors.insert(name.rsplit("::").next()?.to_string());
    for member in item.assoc_item_list()?.assoc_items() {
        let ast::AssocItem::Fn(method) = member else {
            continue;
        };
        if !method.name().is_some_and(|name| name.text() == "source") {
            continue;
        }
        let block = method.body()?.stmt_list()?;
        let first = match block.statements().next() {
            Some(ast::Stmt::ExprStmt(stmt)) => stmt.expr(),
            Some(_) => continue,
            None => block.tail_expr(),
        }?;
        let ast::Expr::MatchExpr(expr) = first else {
            continue;
        };
        for arm in expr.match_arm_list()?.arms() {
            if let Some(ast::Expr::PathExpr(path)) = arm.expr() {
                return Some(Fact {
                    path: name.clone(),
                    probe: InternalErrorTypeProbeId::InternalNested001,
                    target: Some(path_label(&path.path()?)),
                    node: item.syntax().clone(),
                    snippet: format!("impl Error for {name} {{ fn source() -> … }}"),
                });
            }
        }
    }
    None
}

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn derives_error(owner: &impl HasAttrs) -> bool {
    owner.attrs().any(|attr| {
        if attr.simple_name().as_deref() != Some("derive") {
            return false;
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        let Some(tree) = meta.token_tree() else {
            return false;
        };
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|element| !element.kind().is_trivia())
            .collect();
        let [_, contents @ .., _] = tokens.as_slice() else {
            return false;
        };
        for group in contents.split(|token| token.to_string() == ",") {
            let text = group.iter().map(ToString::to_string).collect::<String>();
            let segments: Vec<_> = text.trim_start_matches("::").split("::").collect();
            if segments.iter().any(|part| {
                part.is_empty()
                    || !part
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '#')
            }) {
                break;
            }
            if segments.last() == Some(&"Error") {
                return true;
            }
        }
        false
    })
}
