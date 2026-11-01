//! Allow findings and the narrow, reason-bearing Verus import exception.
use super::{context, tokens};
use crate::etiquettes::allows::{AllowRuleId, AllowSiteRecord};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{AstNode, AstToken, SyntaxElement, ast};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<AllowSiteRecord>> {
    let mut out = Vec::new();
    for attr in file
        .tree()
        .syntax()
        .descendants()
        .filter(super::outside_macro)
        .filter_map(ast::Attr::cast)
    {
        let Some(owner) = super::policy_owner(&attr) else {
            continue;
        };
        if owner
            .ancestors()
            .skip(1)
            .filter_map(ast::Module::cast)
            .any(|module| super::attributes(module.syntax()).any(|attr| super::test_gate(&attr)))
        {
            continue;
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            continue;
        };
        if !meta
            .path()
            .is_some_and(|path| tokens::render(path.syntax()) == "allow")
        {
            continue;
        }
        let Some(tree) = meta.token_tree() else {
            continue;
        };
        let verus = ast::Use::cast(owner.clone())
            .and_then(|item| item.use_tree())
            .is_some_and(|tree| verus_tree(&tree));
        let reason = reason(&tree);
        let rule = if verus {
            if reason.is_some_and(|reason| !reason.trim().is_empty()) {
                continue;
            }
            AllowRuleId::VerusReason001
        } else {
            AllowRuleId::Attr001
        };
        let rendered = tokens::render(tree.syntax());
        let body = rendered
            .strip_prefix('(')
            .and_then(|body| body.strip_suffix(')'))
            .unwrap_or(&rendered);
        let body = body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" :: ", "::");
        let snippet = if body.is_empty() {
            "allow".into()
        } else {
            format!("allow({body})")
        };
        out.push(
            AllowSiteRecord::builder()
                .rule_id(rule)
                .context(context::site(&owner, prefix, true))
                .file(file.path().strip_prefix(root)?.to_path_buf())
                .line(file.line(attr.syntax().text_range().start()))
                .snippet(tokens::truncate(snippet, 96))
                .build()?,
        );
    }
    Ok(out)
}

#[tracing::instrument(level = "trace", skip(tree))]
fn verus_tree(tree: &ast::UseTree) -> bool {
    if let Some(path) = tree.path() {
        let mut root = path;
        while let Some(qualifier) = root.qualifier() {
            root = qualifier;
        }
        return root
            .segment()
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| {
                matches!(
                    name.text(),
                    "vstd" | "verus_builtin" | "verus_builtin_macros"
                )
            });
    }
    tree.use_tree_list()
        .is_some_and(|list| list.use_trees().any(|tree| verus_tree(&tree)))
}

#[tracing::instrument(level = "trace", skip(tree))]
fn reason(tree: &ast::TokenTree) -> Option<String> {
    let mut reason = None;
    let mut entry = Vec::new();
    for element in tree
        .syntax()
        .children_with_tokens()
        .skip(1)
        .filter(|element| !element.kind().is_trivia())
    {
        if matches!(
            element.kind(),
            ra_ap_syntax::SyntaxKind::COMMA | ra_ap_syntax::SyntaxKind::R_PAREN
        ) {
            if let [
                SyntaxElement::Token(name),
                SyntaxElement::Token(eq),
                SyntaxElement::Token(literal),
            ] = entry.as_slice()
                && name.text() == "reason"
                && eq.text() == "="
                && let Some(literal) = ast::String::cast(literal.clone())
                && let Ok(value) = literal.value()
            {
                reason = Some(value.into_owned());
            }
            entry.clear();
        } else {
            entry.push(element);
        }
    }
    reason
}
