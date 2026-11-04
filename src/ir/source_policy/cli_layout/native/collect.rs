//! Collect CLI declarations and external module links before resolving dispatch.
use super::super::{
    EntryItem, EntryKind,
    catalog::{FreeFnRec, TypeRec},
};
use super::{NativeFile, syntax};
use crate::{CordialError, CordialResult, FileSnapshot, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
pub(super) fn file(file: &DatabaseSource<'_>, name: &str) -> CordialResult<NativeFile> {
    let mut result = NativeFile::new(
        Vec::new(),
        BTreeSet::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
    );
    walk(file.tree().syntax(), file, name, &mut result)?;
    for item in file.tree().items() {
        let (name, kind) = match &item {
            ast::Item::Fn(f) => {
                let name = f
                    .name()
                    .map(|n| n.text().to_string())
                    .ok_or_else(|| CordialError::not_found("CLI function name".into()))?;
                let kind = if name == "main" {
                    main_shape(f)
                } else {
                    EntryKind::Function
                };
                (name, kind)
            }
            ast::Item::Module(m) => (
                m.name()
                    .map(|n| n.text().to_string())
                    .ok_or_else(|| CordialError::not_found("CLI module name".into()))?,
                EntryKind::Module,
            ),
            ast::Item::Struct(s) => (
                s.name()
                    .map(|n| n.text().to_string())
                    .ok_or_else(|| CordialError::not_found("CLI struct name".into()))?,
                EntryKind::Struct,
            ),
            ast::Item::Enum(e) => (
                e.name()
                    .map(|n| n.text().to_string())
                    .ok_or_else(|| CordialError::not_found("CLI enum name".into()))?,
                EntryKind::Enum,
            ),
            ast::Item::Impl(_) => (String::new(), EntryKind::Impl),
            _ => continue,
        };
        result.entries.push(EntryItem::new(
            file.path().to_path_buf(),
            file.line(item.syntax().text_range().start()),
            name,
            kind,
        ));
    }
    Ok(result)
}
#[tracing::instrument(level = "debug", skip(node, file, out), err(level = "warn"))]
fn walk(
    node: &SyntaxNode,
    file: &DatabaseSource<'_>,
    name: &str,
    out: &mut NativeFile,
) -> CordialResult<()> {
    if crate::ir::source_policy::native::attributes(node)
        .any(|a| crate::ir::source_policy::native::test_gate(&a))
    {
        return Ok(());
    }
    if let Some(item) = ast::Item::cast(node.clone()) {
        let line = file.line(node.text_range().start());
        match item {
            ast::Item::Struct(item) => {
                let ident = item
                    .name()
                    .ok_or_else(|| CordialError::not_found("CLI struct name".into()))?
                    .text()
                    .to_string();
                out.types.push(type_rec(
                    &ident,
                    name,
                    file,
                    "struct",
                    &item,
                    syntax::fields(item.field_list()),
                    BTreeMap::new(),
                )?);
            }
            ast::Item::Enum(item) => {
                let ident = item
                    .name()
                    .ok_or_else(|| CordialError::not_found("CLI enum name".into()))?
                    .text()
                    .to_string();
                let mut variants = BTreeMap::new();
                if let Some(list) = item.variant_list() {
                    for variant in list.variants() {
                        if let Some(name) = variant.name() {
                            variants.insert(
                                name.text().to_string(),
                                syntax::variant(variant.field_list()),
                            );
                        }
                    }
                }
                out.types.push(type_rec(
                    &ident,
                    name,
                    file,
                    "enum",
                    &item,
                    BTreeMap::new(),
                    variants,
                )?);
            }
            ast::Item::Fn(f) => {
                let ident = f
                    .name()
                    .ok_or_else(|| CordialError::not_found("CLI function name".into()))?
                    .text()
                    .to_string();
                let args = f
                    .param_list()
                    .into_iter()
                    .flat_map(|p| p.params())
                    .filter_map(|p| p.ty())
                    .flat_map(|t| syntax::type_names(&t))
                    .collect();
                out.free_fns.push(FreeFnRec::new(
                    ident,
                    file.path().to_path_buf(),
                    line,
                    false,
                    args,
                ));
            }
            ast::Item::Impl(item) => {
                let ident = item
                    .self_ty()
                    .map(|t| syntax::type_label(&t))
                    .unwrap_or_else(|| "?".into())
                    .rsplit("::")
                    .next()
                    .unwrap_or("?")
                    .to_string();
                if let Some(trait_ty) = item.trait_() {
                    if syntax::type_label(&trait_ty).rsplit("::").next() == Some("Error") {
                        out.error_impls.insert(ident);
                    }
                    return Ok(());
                }
                if let Some(list) = item.assoc_item_list() {
                    for item in list.assoc_items() {
                        let ast::AssocItem::Fn(method) = item else {
                            continue;
                        };
                        if method.name().is_none_or(|n| n.text() != "act") {
                            continue;
                        }
                        let Some(params) = method.param_list() else {
                            continue;
                        };
                        let receiver = params.self_param().is_some()
                            || params
                                .params()
                                .filter_map(|p| p.ty())
                                .any(|t| matches!(t,ast::Type::PathType(ref p) if p.path().is_some_and(|p| syntax::path_names(&p)==["Self"])));
                        let result = method
                            .ret_type()
                            .and_then(|t| t.ty())
                            .map(|t| syntax::type_label(&t))
                            .is_some_and(|t| {
                                t.rsplit("::").next().is_some_and(|n| n.ends_with("Result"))
                            });
                        if receiver
                            && result
                            && let Some(body) = method.body()
                        {
                            out.acts.push((
                                ident.clone(),
                                file.path().to_path_buf(),
                                file.line(method.syntax().text_range().start()),
                                body,
                            ));
                        }
                    }
                }
                return Ok(());
            }
            ast::Item::Module(module)
                if module.semicolon_token().is_some()
                    && !module
                        .syntax()
                        .ancestors()
                        .any(|n| ast::BlockExpr::can_cast(n.kind())) =>
            {
                if let Some(child) = module_path(file.path(), &module)? {
                    out.children.push(child);
                }
            }
            ast::Item::MacroCall(_) => return Ok(()),
            _ => {}
        }
    }
    for child in node.children() {
        walk(&child, file, name, out)?;
    }
    Ok(())
}
#[tracing::instrument(
    level = "trace",
    skip(file, item, fields, variants),
    err(level = "warn")
)]
fn type_rec(
    ident: &str,
    name: &str,
    file: &DatabaseSource<'_>,
    kind: &str,
    item: &impl HasAttrs,
    fields: BTreeMap<String, Vec<String>>,
    variants: BTreeMap<String, super::super::catalog::VariantShape>,
) -> CordialResult<TypeRec> {
    let derives: BTreeSet<_> = item
        .attrs()
        .filter(|a| a.simple_name().as_deref() == Some("derive"))
        .flat_map(|attr| {
            attr.meta()
                .and_then(|meta| match meta {
                    ast::Meta::TokenTreeMeta(meta) => meta.token_tree(),
                    _ => None,
                })
                .map(|t| {
                    t.syntax()
                        .descendants_with_tokens()
                        .filter_map(|e| e.into_token())
                        .filter(|t| t.kind() == ra_ap_syntax::SyntaxKind::IDENT)
                        .map(|t| t.text().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    TypeRec::builder()
        .ident(ident.into())
        .type_path(format!("{name}::{ident}"))
        .file(file.path().to_path_buf())
        .line(file.line(item.syntax().text_range().start()))
        .snippet(format!("{kind} {ident}"))
        .parser(derives.contains("Parser"))
        .subcommand(derives.contains("Subcommand"))
        .error(derives.contains("Error"))
        .in_library(false)
        .fields(fields)
        .variants(variants)
        .build()
}
#[tracing::instrument(level = "trace", skip(function))]
fn main_shape(function: &ast::Fn) -> EntryKind {
    let mut has_match = false;
    let mut has_parse = false;
    let mut has_act = false;
    if let Some(body) = function.body() {
        for node in body.syntax().descendants() {
            if node.ancestors().any(|n| ast::TokenTree::can_cast(n.kind())) {
                continue;
            }
            has_match |= ast::MatchExpr::can_cast(node.kind());
            let method = ast::MethodCallExpr::cast(node.clone())
                .and_then(|c| c.name_ref())
                .map(|n| n.text().to_string());
            let call = ast::CallExpr::cast(node)
                .and_then(|c| c.expr())
                .and_then(|e| match e {
                    ast::Expr::PathExpr(p) => p.path(),
                    _ => None,
                })
                .and_then(|p| syntax::path_names(&p).last().cloned());
            for name in [method, call].into_iter().flatten() {
                has_parse |= name == "parse";
                has_act |= name == "act";
            }
        }
    }
    EntryKind::Main {
        has_match,
        has_parse,
        has_act,
    }
}
#[tracing::instrument(level = "trace", skip(module, file), err(level = "warn"))]
fn module_path(file: &Path, module: &ast::Module) -> CordialResult<Option<PathBuf>> {
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
                    .map_err(|_| CordialError::not_found("valid CLI module path".into()))?
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
