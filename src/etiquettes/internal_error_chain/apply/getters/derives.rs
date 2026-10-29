//! Resolve the supported derive names from explicit file-level imports.

use syn::{Item, Path, UseTree};
use tracing::instrument;

#[instrument(level = "trace", skip(syntax, path))]
pub(in crate::etiquettes::internal_error_chain::apply) fn known_derive(
    syntax: &syn::File,
    path: &Path,
    package: &str,
    name: &str,
) -> bool {
    if path
        .segments
        .iter()
        .any(|segment| !segment.arguments.is_empty())
    {
        return false;
    }
    let segments: Vec<_> = path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect();
    if syntax.items.iter().any(|item| {
        matches!(item, Item::Mod(item) if item.ident == package)
            || matches!(item, Item::ExternCrate(item)
                if item.rename.as_ref().map(|(_, alias)| alias).unwrap_or(&item.ident) == package)
    }) {
        return false;
    }
    let mut root_imports = Vec::new();
    for item in &syntax.items {
        if let Item::Use(item) = item {
            collect_imports(&item.tree, &mut Vec::new(), package, &mut root_imports);
        }
    }
    if !root_imports.is_empty() {
        return false;
    }
    if segments == [package, name] {
        return true;
    }
    let [binding] = segments.as_slice() else {
        return false;
    };
    let mut imports = Vec::new();
    for item in &syntax.items {
        if let Item::Use(item) = item {
            if !item.attrs.is_empty() {
                let mut conditional = Vec::new();
                collect_imports(&item.tree, &mut Vec::new(), binding, &mut conditional);
                if !conditional.is_empty() {
                    return false;
                }
                continue;
            }
            collect_imports(&item.tree, &mut Vec::new(), binding, &mut imports);
        }
        if matches!(item, Item::Macro(item) if item.ident.as_ref().is_some_and(|ident| ident == binding))
        {
            return false;
        }
    }
    imports.as_slice() == [vec![package.to_string(), name.to_string()]]
}

#[instrument(level = "trace", skip(tree, prefix, imports))]
fn collect_imports(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    binding: &str,
    imports: &mut Vec<Vec<String>>,
) {
    match tree {
        UseTree::Path(part) => {
            prefix.push(part.ident.to_string());
            collect_imports(&part.tree, prefix, binding, imports);
            prefix.pop();
        }
        UseTree::Name(part) if part.ident == binding => {
            let mut path = prefix.clone();
            path.push(part.ident.to_string());
            imports.push(path);
        }
        UseTree::Rename(part) if part.rename == binding => {
            let mut path = prefix.clone();
            path.push(part.ident.to_string());
            imports.push(path);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_imports(item, prefix, binding, imports);
            }
        }
        UseTree::Glob(_) => imports.push(Vec::new()),
        _ => {}
    }
}
