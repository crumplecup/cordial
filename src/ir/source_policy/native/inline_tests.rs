//! Inline-test evidence from canonical syntax, without evaluating cfg predicates.

use crate::etiquettes::inline_tests::{InlineTestRuleId, InlineTestSiteRecord};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<InlineTestSiteRecord>> {
    let mut out = Vec::new();
    for node in file
        .tree()
        .syntax()
        .descendants()
        .filter(|node| {
            ast::Module::can_cast(node.kind())
                || ast::Fn::can_cast(node.kind())
                || ast::Impl::can_cast(node.kind())
                || ast::Use::can_cast(node.kind())
        })
        .filter(super::outside_macro)
    {
        if test_container_ancestor(&node) {
            continue;
        }
        let candidate = if let Some(module) = ast::Module::cast(node.clone()) {
            let name = declaration_name(&module)?;
            cfg_attr(&node).map(|attr| {
                (
                    InlineTestRuleId::Mod001,
                    format!("#[cfg(test)] mod {name}"),
                    attr,
                )
            })
        } else if let Some(function) = ast::Fn::cast(node.clone()) {
            if associated_owner(&node).is_some_and(|parent| ast::Trait::can_cast(parent.kind())) {
                continue;
            }
            let name = declaration_name(&function)?;
            cfg_attr(&node)
                .map(|attr| {
                    (
                        InlineTestRuleId::Cfg001,
                        format!("#[cfg(test)] fn {name}"),
                        attr,
                    )
                })
                .or_else(|| {
                    test_attr(&node)
                        .map(|attr| (InlineTestRuleId::Fn001, format!("#[test] fn {name}"), attr))
                })
        } else if ast::Impl::can_cast(node.kind()) {
            cfg_attr(&node).map(|attr| {
                (
                    InlineTestRuleId::Cfg001,
                    "#[cfg(test)] impl <impl>".into(),
                    attr,
                )
            })
        } else if ast::Use::can_cast(node.kind()) {
            cfg_attr(&node).map(|attr| {
                (
                    InlineTestRuleId::Cfg001,
                    "#[cfg(test)] use <use>".into(),
                    attr,
                )
            })
        } else {
            None
        };
        if let Some((rule, snippet, attr)) = candidate {
            out.push(
                InlineTestSiteRecord::builder()
                    .rule_id(rule)
                    .context(context(&node, prefix))
                    .file(file.path().strip_prefix(root)?.to_path_buf())
                    .line(file.line(attr.syntax().text_range().start()))
                    .snippet(snippet)
                    .build()?,
            );
        }
    }
    Ok(out)
}

#[tracing::instrument(level = "trace", skip(owner), err(level = "warn"))]
fn declaration_name(owner: &impl HasName) -> CordialResult<String> {
    owner
        .name()
        .map(|name| name.text().to_string())
        .ok_or_else(|| crate::CordialError::not_found("source declaration name".into()))
}

#[tracing::instrument(level = "trace", skip(node))]
fn cfg_attr(node: &SyntaxNode) -> Option<ast::Attr> {
    super::attributes(node).find(super::test_gate)
}

#[tracing::instrument(level = "trace", skip(node))]
fn test_attr(node: &SyntaxNode) -> Option<ast::Attr> {
    super::attributes(node).find(|attr| {
        let Some(path) = attr.path() else {
            return false;
        };
        let mut parts = Vec::new();
        let mut current = Some(path);
        while let Some(path) = current {
            if let Some(name) = path.segment().and_then(|segment| segment.name_ref()) {
                parts.push(name.text().to_string());
            }
            current = path.qualifier();
        }
        match parts.as_slice() {
            [name] => name == "test" || name == "rstest",
            [name, _] => name == "test",
            _ => false,
        }
    })
}

#[tracing::instrument(level = "trace", skip(node))]
fn test_container_ancestor(node: &SyntaxNode) -> bool {
    node.ancestors().skip(1).any(|parent| {
        (ast::Module::can_cast(parent.kind()) || ast::Impl::can_cast(parent.kind()))
            && cfg_attr(&parent).is_some()
    })
}

#[tracing::instrument(level = "trace", skip(node))]
fn associated_owner(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.parent()
        .filter(|parent| parent.kind() == ra_ap_syntax::SyntaxKind::ASSOC_ITEM_LIST)
        .and_then(|parent| parent.parent())
}

#[tracing::instrument(level = "trace", skip(node, prefix))]
fn context(node: &SyntaxNode, prefix: &[String]) -> String {
    let mut lineage: Vec<_> = node.ancestors().collect();
    lineage.reverse();
    let mut parts = prefix.to_vec();
    for owner in lineage {
        if let Some(module) = ast::Module::cast(owner.clone()) {
            parts.extend(module.name().map(|name| name.text().to_string()));
        } else if let Some(function) = ast::Fn::cast(owner.clone())
            && associated_owner(&owner).is_none()
        {
            parts.extend(function.name().map(|name| name.text().to_string()));
        }
    }
    if parts.is_empty() {
        "<crate>".into()
    } else {
        parts.join("::")
    }
}
