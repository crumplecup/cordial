//! Canonical module declarations select files without reparsing source.
use crate::{CordialError, CordialResult};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};
use std::path::{Path, PathBuf};

/// Where a `mod name;` declaration points, as far as the file's syntax and path say: which files it
/// may be, and not which of them exists.
///
/// That is the part of finding a module's file that reads only the syntax, so it can be kept with
/// the file; whether a file exists is a question of the files the database holds, asked where the
/// target is followed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ModuleTarget {
    /// The file a `#[path = "..."]` names.
    Explicit(PathBuf),
    /// A `#[path]` that is not a string literal that can be read.
    Invalid,
    /// `name.rs` or `name/mod.rs` in the directory the module's ancestors make.
    Default { flat: PathBuf, nested: PathBuf },
    /// The file has no directory, or the declaration no name.
    Nowhere,
}

impl ModuleTarget {
    /// The file the declaration leads to, if it exists on the file system.
    #[tracing::instrument(level = "trace", skip(self), err(level = "warn"))]
    pub(crate) fn resolve(&self) -> CordialResult<Option<PathBuf>> {
        match self {
            Self::Explicit(path) => Ok(path.is_file().then(|| path.clone())),
            Self::Invalid => Err(CordialError::not_found("valid module path".into())),
            Self::Default { flat, nested } => {
                if flat.is_file() {
                    Ok(Some(flat.clone()))
                } else {
                    Ok(nested.is_file().then(|| nested.clone()))
                }
            }
            Self::Nowhere => Ok(None),
        }
    }
}

/// Where `module`, declared in `file`, points.
#[tracing::instrument(level = "trace", skip(module, file))]
pub(crate) fn module_target(file: &Path, module: &ast::Module) -> ModuleTarget {
    let Some(parent) = file.parent() else {
        return ModuleTarget::Nowhere;
    };
    for attr in module
        .attrs()
        .filter(|a| a.simple_name().as_deref() == Some("path"))
    {
        if let Some(ast::Meta::KeyValueMeta(meta)) = attr.meta()
            && let Some(ast::Expr::Literal(literal)) = meta.expr()
            && let ast::LiteralKind::String(string) = literal.kind()
        {
            let Ok(value) = string.value() else {
                return ModuleTarget::Invalid;
            };
            let path = PathBuf::from(value.as_ref());
            return ModuleTarget::Explicit(if path.is_absolute() {
                path
            } else {
                parent.join(path)
            });
        }
    }
    let Some(name) = module.name() else {
        return ModuleTarget::Nowhere;
    };
    let mut dir = if file
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| matches!(s, "lib" | "main" | "mod"))
    {
        parent.to_path_buf()
    } else {
        file.with_extension("")
    };
    let mut inline = module
        .syntax()
        .ancestors()
        .skip(1)
        .filter_map(ast::Module::cast)
        .filter_map(|m| m.name())
        .map(|n| n.text().to_string())
        .collect::<Vec<_>>();
    inline.reverse();
    for part in inline {
        dir.push(part);
    }
    ModuleTarget::Default {
        flat: dir.join(format!("{}.rs", name.text())),
        nested: dir.join(name.text()).join("mod.rs"),
    }
}

/// The files a file's out-of-line `mod name;` declarations may lead to, in the order they are
/// declared: through the inline modules of the file, and not through those a configuration
/// excludes.
#[tracing::instrument(level = "trace", skip(items, file))]
pub(crate) fn declared_modules(
    items: impl Iterator<Item = ast::Item>,
    file: &Path,
) -> Vec<ModuleTarget> {
    let mut found = Vec::new();
    for module in items.filter_map(|item| match item {
        ast::Item::Module(module) => Some(module),
        _ => None,
    }) {
        if crate::ir::excluded_module(module.syntax()) {
            continue;
        }
        match module.item_list() {
            Some(list) => found.extend(declared_modules(list.items(), file)),
            None => found.push(module_target(file, &module)),
        }
    }
    found
}

#[tracing::instrument(level = "trace", skip(module, file), err(level = "warn"))]
pub(crate) fn module_path(file: &Path, module: &ast::Module) -> CordialResult<Option<PathBuf>> {
    module_target(file, module).resolve()
}
