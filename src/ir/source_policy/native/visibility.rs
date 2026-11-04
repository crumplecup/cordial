//! Visibility evidence from canonical Rust syntax, including inactive modules.
use super::super::{
    SourcePolicyInventory,
    visibility::{self, VisibilityBody, VisibilityChild, VisibilityChildBody},
};
use crate::{
    CordialError, CordialResult, FileSnapshot,
    ir::{VisibilityFacts, VisibilityKind},
    loader::DatabaseSource,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasModuleItem, HasName, HasVisibility},
};
use std::path::Path;

impl SourcePolicyInventory {
    #[tracing::instrument(level = "debug", skip(read), err(level = "warn"))]
    pub(crate) fn collect_native_visibility(
        root: &Path,
        read: impl FnMut(&Path) -> CordialResult<(VisibilityBody, FileSnapshot)>,
    ) -> CordialResult<VisibilityFacts> {
        visibility::collect(root, read)
    }

    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    pub(crate) fn native_visibility_file(
        file: &DatabaseSource<'_>,
    ) -> CordialResult<(VisibilityBody, FileSnapshot)> {
        Ok((
            items(file.tree().items(), file)?,
            FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
        ))
    }
}

#[tracing::instrument(level = "debug", skip(input, file), err(level = "warn"))]
fn items(
    input: impl Iterator<Item = ast::Item>,
    file: &DatabaseSource<'_>,
) -> CordialResult<VisibilityBody> {
    let (mut leaf_pub, mut leaf_crate, mut children) = (0, 0, Vec::new());
    for item in input {
        if let ast::Item::Module(module) = &item {
            if excluded(module) {
                continue;
            }
            let name = module
                .name()
                .ok_or_else(|| CordialError::not_found("visibility module name".into()))?
                .text()
                .to_string();
            let content = if let Some(list) = module.item_list() {
                VisibilityChildBody::Inline(items(list.items(), file)?)
            } else {
                VisibilityChildBody::External(path_attr(module)?)
            };
            children.push(VisibilityChild::new(
                name,
                file.line(module.syntax().text_range().start()),
                kind(module.visibility()),
                content,
            ));
        } else if let ast::Item::MacroCall(invocation) = &item {
            #[cfg(feature = "verus_ir")]
            if invocation
                .path()
                .is_some_and(|path| super::tokens::render(path.syntax()) == "verus")
            {
                // Rust's token tree cannot establish Verus declarations. No parser fallback.
                return Err(CordialError::not_found(
                    format!(
                        "native Verus visibility evidence: {}",
                        file.path().display()
                    )
                    .into(),
                ));
            }
            let _ = invocation;
        } else {
            let count = match &item {
                ast::Item::Use(item) => item.use_tree().map(use_names).unwrap_or(0),
                ast::Item::Const(_)
                | ast::Item::Enum(_)
                | ast::Item::Fn(_)
                | ast::Item::Static(_)
                | ast::Item::Struct(_)
                | ast::Item::Trait(_)
                | ast::Item::TypeAlias(_)
                | ast::Item::Union(_) => 1,
                _ => 0,
            };
            let visibility = ast::AnyHasVisibility::cast(item.syntax().clone())
                .and_then(|item| item.visibility());
            match kind(visibility) {
                VisibilityKind::Pub => {
                    leaf_pub += count;
                    leaf_crate += count;
                }
                VisibilityKind::PubCrate => leaf_crate += count,
                _ => {}
            }
        }
    }
    Ok(VisibilityBody::new(leaf_pub, leaf_crate, children))
}

#[tracing::instrument(level = "trace", skip(module))]
fn excluded(module: &ast::Module) -> bool {
    // Inner attributes belong to syn's module attrs as well as outer attributes.
    super::attributes(module.syntax()).any(|attr| {
        if super::test_gate(&attr) {
            return true;
        }
        if attr.simple_name().as_deref() != Some("doc") {
            return false;
        }
        match attr.meta() {
            Some(ast::Meta::TokenTreeMeta(meta)) => meta.token_tree().is_some_and(|tree| {
                super::tokens::render(tree.syntax())
                    .replace(' ', "")
                    .contains("hidden")
            }),
            _ => false,
        }
    })
}

#[tracing::instrument(level = "trace", skip(module), err(level = "warn"))]
fn path_attr(module: &ast::Module) -> CordialResult<Option<String>> {
    for attr in module.attrs() {
        if attr.simple_name().as_deref() != Some("path") {
            continue;
        }
        if let Some(ast::Meta::KeyValueMeta(meta)) = attr.meta()
            && let Some(ast::Expr::Literal(literal)) = meta.expr()
            && let ast::LiteralKind::String(string) = literal.kind()
        {
            return Ok(Some(
                string
                    .value()
                    .map_err(|_| CordialError::not_found("valid visibility path literal".into()))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

#[tracing::instrument(level = "trace", skip(visibility))]
fn kind(visibility: Option<ast::Visibility>) -> VisibilityKind {
    match visibility.map(|visibility| visibility.kind()) {
        None => VisibilityKind::Private,
        Some(ast::VisibilityKind::Pub) => VisibilityKind::Pub,
        Some(ast::VisibilityKind::PubSuper) => VisibilityKind::PubSuper,
        _ => VisibilityKind::PubCrate,
    }
}

#[tracing::instrument(level = "trace", skip(tree))]
fn use_names(tree: ast::UseTree) -> usize {
    if let Some(list) = tree.use_tree_list() {
        list.use_trees().map(use_names).sum()
    } else if tree.star_token().is_some() {
        0
    } else {
        1
    }
}
