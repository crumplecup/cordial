//! Explicit derive bindings, preserving ambiguity and conditional-import guards.
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasAttrs, HasGenericArgs, HasModuleItem, HasName},
};

#[tracing::instrument(level = "trace", skip(file, owner))]
pub(in crate::etiquettes::internal_error_chain::apply) fn has_derive(
    file: &DatabaseSource<'_>,
    owner: &impl HasAttrs,
    package: &str,
    name: &str,
) -> bool {
    owner
        .attrs()
        .filter(|attr| super::attrs::named(attr, "derive"))
        .filter_map(|attr| paths(&attr))
        .any(|paths| paths.iter().any(|path| known(file, path, package, name)))
}

#[tracing::instrument(level = "trace", skip(file, owner))]
pub(super) fn has_display(file: &DatabaseSource<'_>, owner: &ast::Struct) -> bool {
    owner
        .attrs()
        .filter(|attr| super::attrs::named(attr, "derive"))
        .filter_map(|attr| paths(&attr))
        .any(|paths| {
            paths.iter().any(|path| {
                path == &["derive_more", "Display"] || known(file, path, "derive_more", "Display")
            })
        })
}

#[tracing::instrument(level = "trace", skip(file, path))]
pub(in crate::etiquettes::internal_error_chain::apply) fn known(
    file: &DatabaseSource<'_>,
    path: &[String],
    package: &str,
    name: &str,
) -> bool {
    for item in file.tree().items() {
        let shadow = match item {
            ast::Item::Module(item) => item.name().is_some_and(|name| name.text() == package),
            ast::Item::ExternCrate(item) => item
                .rename()
                .map(|rename| rename.name().is_some_and(|name| name.text() == package))
                .unwrap_or_else(|| item.name_ref().is_some_and(|name| name.text() == package)),
            _ => false,
        };
        if shadow {
            return false;
        }
    }
    if !imports(file, package).is_empty() {
        return false;
    }
    if path == [package, name] {
        return true;
    }
    let [binding] = path else { return false };
    for item in file.tree().items() {
        match item {
            ast::Item::Use(item) if item.attrs_with_doc().next().is_some() => {
                let mut found = Vec::new();
                if let Some(tree) = item.use_tree() {
                    bindings(&tree, Vec::new(), binding, &mut found);
                }
                if !found.is_empty() {
                    return false;
                }
            }
            ast::Item::MacroRules(item)
                if item.name().is_some_and(|name| name.text() == binding) =>
            {
                return false;
            }
            _ => {}
        }
    }
    imports(file, binding).as_slice() == [vec![package.to_string(), name.to_string()]]
}

#[tracing::instrument(level = "trace", skip(file))]
fn imports(file: &DatabaseSource<'_>, binding: &str) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    for item in file.tree().items() {
        if let ast::Item::Use(item) = item
            && let Some(tree) = item.use_tree()
        {
            bindings(&tree, Vec::new(), binding, &mut found);
        }
    }
    found
}

#[tracing::instrument(level = "trace", skip(tree, found))]
fn bindings(
    tree: &ast::UseTree,
    mut prefix: Vec<String>,
    binding: &str,
    found: &mut Vec<Vec<String>>,
) {
    if let Some(path) = tree.path().and_then(|path| segments(&path)) {
        prefix.extend(path);
    }
    if let Some(group) = tree.use_tree_list() {
        for child in group.use_trees() {
            bindings(&child, prefix.clone(), binding, found);
        }
    } else if tree.star_token().is_some() {
        found.push(Vec::new());
    } else {
        let name = tree
            .rename()
            .map(|rename| {
                rename
                    .name()
                    .map(|name| name.text().to_string())
                    .or_else(|| rename.underscore_token().map(|_| "_".to_string()))
            })
            .unwrap_or_else(|| prefix.last().cloned());
        if name.as_deref() == Some(binding) {
            found.push(prefix);
        }
    }
}

#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn segments(path: &ast::Path) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut current = Some(path.clone());
    while let Some(path) = current {
        let segment = path.segment()?;
        if segment.generic_arg_list().is_some()
            || segment.parenthesized_arg_list().is_some()
            || segment.type_anchor().is_some()
        {
            return None;
        }
        names.push(segment.name_ref()?.text().to_string());
        current = path.qualifier();
    }
    names.reverse();
    Some(names)
}

/// Derive paths occupy canonical token-tree arguments, with no nested options.
#[tracing::instrument(level = "trace", skip(attr))]
pub(in crate::etiquettes::internal_error_chain::apply) fn paths(
    attr: &ast::Attr,
) -> Option<Vec<Vec<String>>> {
    let ast::Meta::TokenTreeMeta(meta) = attr.meta()? else {
        return None;
    };
    let tree = meta.token_tree()?;
    tree.l_paren_token()?;
    let tokens: Vec<_> = tree
        .syntax()
        .children_with_tokens()
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let [_, inner @ .., _] = tokens.as_slice() else {
        return None;
    };
    let mut paths = Vec::new();
    let mut groups = inner
        .split(|token| token.kind() == SyntaxKind::COMMA)
        .peekable();
    while let Some(group) = groups.next() {
        if group.is_empty() && groups.peek().is_none() {
            continue;
        }
        let mut names = Vec::new();
        let mut index = separator(group, 0).unwrap_or(0);
        loop {
            let name = group.get(index)?.as_token()?;
            if name.kind() != SyntaxKind::IDENT
                && !matches!(name.text(), "self" | "Self" | "super" | "crate")
            {
                return None;
            }
            names.push(name.text().to_string());
            index += 1;
            if index == group.len() {
                break;
            }
            index = separator(group, index)?;
        }
        paths.push(names);
    }
    Some(paths)
}

#[tracing::instrument(level = "trace", skip(tokens))]
fn separator(tokens: &[ra_ap_syntax::SyntaxElement], index: usize) -> Option<usize> {
    match tokens.get(index)?.kind() {
        SyntaxKind::COLON2 => Some(index + 1),
        SyntaxKind::COLON if tokens.get(index + 1)?.kind() == SyntaxKind::COLON => Some(index + 2),
        _ => None,
    }
}
