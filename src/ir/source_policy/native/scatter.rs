//! Raw scatter groups; thresholds are applied later from the shared inventory.
use super::{context, tokens};
use crate::etiquettes::cfg_scatter::{CfgScatterGroup, CfgSiteKind, CfgSiteOccurrence};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};
use std::{collections::BTreeMap, path::Path};

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<CfgScatterGroup>> {
    let mut groups: BTreeMap<String, Vec<CfgSiteOccurrence>> = BTreeMap::new();
    for owner in file
        .tree()
        .syntax()
        .descendants()
        .filter(super::outside_macro)
    {
        let Some((kind, snippet)) = site(&owner) else {
            continue;
        };
        let line = ast::AnyHasAttrs::cast(owner.clone())
            .and_then(|owner| ast::attrs_with_doc_including_inner(&owner).next())
            .map(|attr| file.line(attr.syntax().text_range().start()))
            .unwrap_or(0);
        for attr in super::attributes(&owner) {
            let Some(ast::Meta::CfgMeta(meta)) = attr.meta() else {
                continue;
            };
            let Some(predicate) = meta.cfg_predicate() else {
                continue;
            };
            let predicate = tokens::render(predicate.syntax())
                .replace(" :: ", "::")
                .replace(" ,", ",");
            if predicate == "test" {
                continue;
            }
            groups.entry(predicate).or_default().push(
                CfgSiteOccurrence::builder()
                    .kind(kind)
                    .context(context::site(&owner, prefix, false))
                    .line(line)
                    .snippet(snippet.clone())
                    .build()?,
            );
        }
    }
    let relative = file.path().strip_prefix(root)?.to_path_buf();
    Ok(groups
        .into_iter()
        .map(|(predicate, sites)| CfgScatterGroup::new(relative.clone(), predicate, sites))
        .collect())
}

#[tracing::instrument(level = "trace", skip(node))]
fn site(node: &SyntaxNode) -> Option<(CfgSiteKind, String)> {
    if let Some(item) = ast::Fn::cast(node.clone()) {
        let container = node.parent().and_then(|parent| parent.parent());
        let kind = match container {
            Some(parent) if ast::Impl::can_cast(parent.kind()) => CfgSiteKind::ImplFn,
            Some(parent) if ast::Trait::can_cast(parent.kind()) => CfgSiteKind::TraitFn,
            _ => CfgSiteKind::Fn,
        };
        if kind == CfgSiteKind::Fn
            && super::attributes(node)
                .filter_map(|attr| attr.meta())
                .any(|meta| {
                    meta.path().is_some_and(|path| {
                        matches!(
                            tokens::render(path.syntax()).as_str(),
                            "proc_macro" | "proc_macro_derive" | "proc_macro_attribute"
                        )
                    })
                })
        {
            return None;
        }
        return item
            .name()
            .map(|name| (kind, format!("fn {}", name.text())));
    }
    if let Some(item) = ast::RecordField::cast(node.clone()) {
        return Some((
            CfgSiteKind::Field,
            format!(
                "field {}",
                item.name()
                    .map(|name| name.text().to_string())
                    .unwrap_or_else(|| "_".into())
            ),
        ));
    }
    if ast::TupleField::can_cast(node.kind()) {
        return Some((CfgSiteKind::Field, "field _".into()));
    }
    if let Some(item) = ast::Variant::cast(node.clone()) {
        return item
            .name()
            .map(|name| (CfgSiteKind::Variant, format!("variant {}", name.text())));
    }
    if ast::MatchArm::can_cast(node.kind()) {
        return Some((CfgSiteKind::Arm, "match arm".into()));
    }
    if ast::Use::can_cast(node.kind()) {
        return Some((CfgSiteKind::Use, "use ...".into()));
    }
    let kind = match node.kind() {
        ra_ap_syntax::SyntaxKind::STRUCT => CfgSiteKind::Struct,
        ra_ap_syntax::SyntaxKind::ENUM => CfgSiteKind::Enum,
        ra_ap_syntax::SyntaxKind::TRAIT => CfgSiteKind::Trait,
        ra_ap_syntax::SyntaxKind::IMPL => CfgSiteKind::Impl,
        ra_ap_syntax::SyntaxKind::CONST => CfgSiteKind::Const,
        ra_ap_syntax::SyntaxKind::STATIC => CfgSiteKind::Static,
        ra_ap_syntax::SyntaxKind::TYPE_ALIAS => CfgSiteKind::TypeAlias,
        _ => return None,
    };
    context::label(node).map(|label| (kind, label))
}
