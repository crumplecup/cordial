//! Glob use trees collected from the canonical syntax, including inactive code.

use crate::etiquettes::glob_imports::{GlobImportRuleId, GlobImportSiteRecord};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<GlobImportSiteRecord>> {
    let mut out = Vec::new();
    for item in file
        .tree()
        .syntax()
        .descendants()
        .filter(super::outside_macro)
        .filter_map(ast::Use::cast)
    {
        let mut modules: Vec<_> = item
            .syntax()
            .ancestors()
            .filter_map(ast::Module::cast)
            .filter_map(|module| module.name().map(|name| name.text().to_string()))
            .collect();
        modules.reverse();
        let mut context = prefix.to_vec();
        context.extend(modules);
        let context = if context.is_empty() {
            "<crate>".into()
        } else {
            context.join("::")
        };
        if let Some(tree) = item.use_tree() {
            collect_tree(&tree, &[], &context, file, root, &mut out)?;
        }
    }
    Ok(out)
}

#[tracing::instrument(level = "trace", skip(tree, prefix, file, out), err(level = "warn"))]
fn collect_tree(
    tree: &ast::UseTree,
    prefix: &[String],
    context: &str,
    file: &DatabaseSource<'_>,
    root: &Path,
    out: &mut Vec<GlobImportSiteRecord>,
) -> CordialResult<()> {
    let mut segments = Vec::new();
    let mut path = tree.path();
    while let Some(current) = path {
        if let Some(segment) = current.segment() {
            let name = segment
                .name_ref()
                .ok_or_else(|| crate::CordialError::not_found("glob import path segment".into()))?;
            segments.push(name.text().to_string());
        }
        path = current.qualifier();
    }
    segments.reverse();
    let mut full = prefix.to_vec();
    full.extend(segments);
    if let Some(star) = tree.star_token()
        && !full.last().is_some_and(|segment| segment == "prelude")
    {
        let snippet = if full.is_empty() {
            "*".into()
        } else {
            format!("{}::*", full.join("::"))
        };
        out.push(
            GlobImportSiteRecord::builder()
                .rule_id(GlobImportRuleId::Import001)
                .context(context.to_string())
                .file(file.path().strip_prefix(root)?.to_path_buf())
                .line(file.line(star.text_range().start()))
                .snippet(snippet)
                .build()?,
        );
    }
    if let Some(list) = tree.use_tree_list() {
        for nested in list.use_trees() {
            collect_tree(&nested, &full, context, file, root, out)?;
        }
    }
    Ok(())
}
