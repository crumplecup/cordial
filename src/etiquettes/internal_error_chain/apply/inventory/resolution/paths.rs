//! Conservative crate module and import path identities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use syn::{Item, UseTree};
use tracing::instrument;

use crate::loader::SharedParsedFile;

#[instrument(level = "debug", skip(files))]
pub(super) fn active_modules(files: &[SharedParsedFile]) -> HashMap<PathBuf, Vec<String>> {
    let mut by_module: HashMap<Vec<String>, Vec<&SharedParsedFile>> = HashMap::new();
    for file in files {
        if let Some(module) = module_path(file.path()) {
            by_module.entry(module).or_default().push(file);
        }
    }
    let mut active = HashMap::new();
    let mut pending = vec![Vec::new()];
    while let Some(module) = pending.pop() {
        let Some([file]) = by_module.get(&module).map(Vec::as_slice) else {
            continue;
        };
        active.insert(file.path().to_path_buf(), module.clone());
        for item in &file.syntax().items {
            let Item::Mod(item) = item else { continue };
            if item.content.is_some() || item.attrs.iter().any(|attr| !attr.path().is_ident("doc"))
            {
                continue;
            }
            let mut child = module.clone();
            child.push(item.ident.to_string());
            if by_module.get(&child).is_some_and(|files| files.len() == 1) {
                pending.push(child);
            }
        }
    }
    active
}

#[instrument(level = "debug", skip(path))]
fn module_path(path: &Path) -> Option<Vec<String>> {
    let src = path
        .ancestors()
        .find(|part| part.file_name().is_some_and(|name| name == "src"))?;
    let relative = path.strip_prefix(src).ok()?;
    let mut parts: Vec<String> = relative
        .iter()
        .map(|part| part.to_str().map(str::to_string))
        .collect::<Option<_>>()?;
    let file = parts.pop()?;
    let stem = file.strip_suffix(".rs")?;
    if !matches!(stem, "lib" | "main" | "mod") {
        parts.push(stem.to_string());
    }
    Some(parts)
}

#[instrument(level = "debug")]
pub(super) fn absolute_path(module: &[String], written: &[String]) -> Option<Vec<String>> {
    let (mut base, mut rest) = if written.first().is_some_and(|name| name == "crate") {
        (Vec::new(), &written[1..])
    } else if written.first().is_some_and(|name| name == "self") {
        (module.to_vec(), &written[1..])
    } else {
        (module.to_vec(), written)
    };
    while rest.first().is_some_and(|name| name == "super") {
        base.pop()?;
        rest = &rest[1..];
    }
    if rest.is_empty() {
        return None;
    }
    base.extend(rest.iter().cloned());
    Some(base)
}

#[instrument(level = "debug", skip(tree))]
pub(super) fn use_paths(
    tree: &UseTree,
    prefix: Vec<String>,
    paths: &mut Vec<(String, Vec<String>)>,
) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            use_paths(&path.tree, prefix, paths);
        }
        UseTree::Name(name) => {
            let mut path = prefix;
            path.push(name.ident.to_string());
            paths.push((name.ident.to_string(), path));
        }
        UseTree::Rename(rename) => {
            let mut path = prefix;
            path.push(rename.ident.to_string());
            paths.push((rename.rename.to_string(), path));
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                use_paths(tree, prefix.clone(), paths);
            }
        }
        UseTree::Glob(_) => {}
    }
}
