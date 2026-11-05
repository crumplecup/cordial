//! Canonical module declarations select files without reparsing source.
use crate::{CordialError, CordialResult};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};
use std::path::{Path, PathBuf};

#[tracing::instrument(level = "trace", skip(module, file), err(level = "warn"))]
pub(crate) fn module_path(file: &Path, module: &ast::Module) -> CordialResult<Option<PathBuf>> {
    let Some(parent) = file.parent() else {
        return Ok(None);
    };
    for attr in module
        .attrs()
        .filter(|a| a.simple_name().as_deref() == Some("path"))
    {
        if let Some(ast::Meta::KeyValueMeta(meta)) = attr.meta()
            && let Some(ast::Expr::Literal(literal)) = meta.expr()
            && let ast::LiteralKind::String(string) = literal.kind()
        {
            let path = PathBuf::from(
                string
                    .value()
                    .map_err(|_| CordialError::not_found("valid module path".into()))?
                    .as_ref(),
            );
            let path = if path.is_absolute() {
                path
            } else {
                parent.join(path)
            };
            return Ok(path.is_file().then_some(path));
        }
    }
    let Some(name) = module.name() else {
        return Ok(None);
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
    let flat = dir.join(format!("{}.rs", name.text()));
    if flat.is_file() {
        return Ok(Some(flat));
    }
    let nested = dir.join(name.text().to_string()).join("mod.rs");
    Ok(nested.is_file().then_some(nested))
}
