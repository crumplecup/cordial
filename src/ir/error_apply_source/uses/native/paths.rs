//! Located path prefixes outside inline modules, including shorthand fields.

use super::{LocatedName, located, opaque_meta};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind, ast};

#[tracing::instrument(level = "trace", skip(file, paths))]
pub(super) fn collect(file: &DatabaseSource<'_>, paths: &mut Vec<(LocatedName, Vec<String>)>) {
    for node in file.tree().syntax().descendants() {
        if node.ancestors().any(|ancestor| {
            opaque_meta(ancestor.kind())
                || matches!(
                    ancestor.kind(),
                    SyntaxKind::MODULE | SyntaxKind::TOKEN_TREE | SyntaxKind::USE_TREE
                )
        }) {
            continue;
        }
        if let Some(path) = ast::Path::cast(node.clone()) {
            if node
                .parent()
                .is_some_and(|parent| ast::Path::can_cast(parent.kind()))
            {
                continue;
            }
            let mut prefix = Vec::new();
            for name in segments(path) {
                let label = name.text().to_string();
                prefix.push(label.clone());
                paths.push((
                    located(file, label, name.syntax().text_range().start()),
                    prefix.clone(),
                ));
            }
        }
        if let Some(field) = ast::RecordExprField::cast(node)
            && field.colon_token().is_none()
            && let Some(name) = field.name_ref()
        {
            let label = name.text().to_string();
            paths.push((
                located(file, label.clone(), name.syntax().text_range().start()),
                vec![label],
            ));
        }
    }
}

#[tracing::instrument(level = "trace", skip(path))]
fn segments(path: ast::Path) -> Vec<ast::NameRef> {
    let mut prefix = path.qualifier().map(segments).unwrap_or_default();
    if let Some(segment) = path.segment() {
        if let Some(name) = segment.name_ref() {
            prefix.push(name);
        } else if let Some(anchor) = segment.type_anchor()
            && anchor.as_token().is_some()
            && let Some(path) = anchor
                .syntax()
                .children()
                .filter_map(ast::PathType::cast)
                .last()
                .and_then(|ty| ty.path())
        {
            prefix.extend(segments(path));
        }
    }
    prefix
}
