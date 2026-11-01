//! Conditional names, including inactive owners and only cfg_attr's predicate.
use super::{context, tokens};
use crate::etiquettes::cfg_hygiene::CfgNameOccurrence;
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{AstNode, ast};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<CfgNameOccurrence>> {
    let mut out = Vec::new();
    for attr in file
        .tree()
        .syntax()
        .descendants()
        .filter(super::outside_macro)
        .filter_map(ast::Attr::cast)
    {
        let predicate = match attr.meta() {
            Some(ast::Meta::CfgMeta(meta)) => meta.cfg_predicate(),
            Some(ast::Meta::CfgAttrMeta(meta)) => meta.cfg_predicate(),
            _ => None,
        };
        let Some(predicate) = predicate else {
            continue;
        };
        let Some(owner) = super::policy_owner(&attr) else {
            continue;
        };
        for name in names(predicate) {
            out.push(
                CfgNameOccurrence::builder()
                    .name(name)
                    .context(context::hygiene(&owner, prefix))
                    .file(file.path().strip_prefix(root)?.to_path_buf())
                    .line(file.line(attr.syntax().text_range().start()))
                    .snippet(tokens::truncate(tokens::render(attr.syntax()), 120))
                    .build()?,
            );
        }
    }
    Ok(out)
}

#[tracing::instrument(level = "trace", skip(predicate))]
fn names(predicate: ast::CfgPredicate) -> Vec<String> {
    match predicate {
        ast::CfgPredicate::CfgAtom(atom) => atom
            .ident_token()
            .map(|name| vec![name.text().to_string()])
            .unwrap_or_default(),
        ast::CfgPredicate::CfgComposite(composite) => {
            let keyword = composite.keyword().map(|token| token.text().to_string());
            match keyword.as_deref() {
                Some("all" | "any" | "not") => composite.cfg_predicates().flat_map(names).collect(),
                Some(name) => vec![name.into()],
                None => Vec::new(),
            }
        }
    }
}
