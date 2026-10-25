//! Conservative crate-local identities for declarations referenced by apply.
//!
//! The index connects facts collected from separate shared source trees. It
//! only resolves a path when the spelling and module location identify one
//! private top-level declaration. Unknown imports and ambiguous names stay
//! unresolved for the conversion planner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use syn::{Item, UseTree, Visibility};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::edges::{ErrorEdgeTarget, ErrorEdgeTargetKind};
use super::helpers::derive_is_builtin;

pub(in crate::etiquettes::internal_error_chain::apply) struct CrateTypeIndex {
    declarations: HashMap<Vec<String>, Vec<Option<ErrorEdgeTarget>>>,
    local: HashMap<(PathBuf, String), Vec<Option<ErrorEdgeTarget>>>,
    active_modules: HashMap<PathBuf, Vec<String>>,
    imports: HashMap<PathBuf, Vec<(String, Vec<String>)>>,
}

impl CrateTypeIndex {
    #[instrument(level = "debug", skip(files))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn new(
        files: &[SharedParsedFile],
    ) -> Self {
        let mut declarations: HashMap<Vec<String>, Vec<Option<ErrorEdgeTarget>>> = HashMap::new();
        let mut local: HashMap<(PathBuf, String), Vec<Option<ErrorEdgeTarget>>> = HashMap::new();
        let active_modules = active_modules(files);
        let mut imports = HashMap::new();
        for file in files {
            let mut file_imports = Vec::new();
            for item in &file.syntax().items {
                if let Item::Use(item) = item {
                    use_paths(&item.tree, Vec::new(), &mut file_imports);
                }
                let (name, visibility, attrs, kind, location) = match item {
                    Item::Struct(item) => (
                        &item.ident,
                        &item.vis,
                        &item.attrs,
                        ErrorEdgeTargetKind::Struct,
                        item.ident.span().start(),
                    ),
                    Item::Enum(item) => (
                        &item.ident,
                        &item.vis,
                        &item.attrs,
                        ErrorEdgeTargetKind::Enum,
                        item.ident.span().start(),
                    ),
                    _ => continue,
                };
                let target = (matches!(visibility, Visibility::Inherited)
                    && attrs.iter().all(derive_is_builtin))
                .then(|| {
                    ErrorEdgeTarget::from_shared_file(file, location.line, location.column, kind)
                });
                local
                    .entry((file.path().to_path_buf(), name.to_string()))
                    .or_default()
                    .push(target.clone());
                if let Some(module) = active_modules.get(file.path()) {
                    let mut key = module.clone();
                    key.push(name.to_string());
                    declarations.entry(key).or_default().push(target);
                }
            }
            imports.insert(file.path().to_path_buf(), file_imports);
        }
        Self {
            declarations,
            local,
            active_modules,
            imports,
        }
    }

    #[instrument(level = "trace", skip(self, file, written_path))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn resolve(
        &self,
        file: &SharedParsedFile,
        written_path: &[String],
    ) -> Option<ErrorEdgeTarget> {
        if let [name] = written_path
            && let Some(candidates) = self.local.get(&(file.path().to_path_buf(), name.clone()))
        {
            return match candidates.as_slice() {
                [Some(target)] => Some(target.clone()),
                _ => None,
            };
        }
        let module = self.active_modules.get(file.path())?;
        let absolute = absolute_path(&module, written_path)?;
        if let Some(target) = self.unique(&absolute) {
            return Some(target);
        }
        if written_path.len() != 1 {
            return None;
        }
        let name = &written_path[0];
        let matches: Vec<_> = self
            .imports
            .get(file.path())?
            .iter()
            .filter(|(alias, _)| alias == name)
            .collect();
        let [(_, path)] = matches.as_slice() else {
            return None;
        };
        if !path
            .first()
            .is_some_and(|part| matches!(part.as_str(), "crate" | "self" | "super"))
        {
            return None;
        }
        self.unique(&absolute_path(&module, path)?)
    }

    #[instrument(level = "trace", skip(self, path))]
    fn unique(&self, path: &[String]) -> Option<ErrorEdgeTarget> {
        let [Some(target)] = self.declarations.get(path)?.as_slice() else {
            return None;
        };
        Some(target.clone())
    }
}

#[instrument(level = "debug", skip(files))]
fn active_modules(files: &[SharedParsedFile]) -> HashMap<PathBuf, Vec<String>> {
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
            if item.content.is_some() || !item.attrs.is_empty() {
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
fn absolute_path(module: &[String], written: &[String]) -> Option<Vec<String>> {
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
fn use_paths(tree: &UseTree, prefix: Vec<String>, paths: &mut Vec<(String, Vec<String>)>) {
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
