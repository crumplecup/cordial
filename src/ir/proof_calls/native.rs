//! Proof call evidence from canonical database trees and memoized macro islands.

use super::{SourceProofFunction, assert_arguments, qualify};
use crate::ir::source_attributes::native_type_key;
use crate::loader::{DatabaseMacroSyntax, DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};

impl SourceProofFunction {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "SourceProofFunction")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>, prefix: &[String]) -> Vec<Self> {
        let mut out = Vec::new();
        let macros = file.proof_macro_syntax();
        items(&macros, file.tree().items(), prefix, &[], &mut out);
        out
    }
}

#[tracing::instrument(level = "debug", skip(file, source, out))]
fn items(
    file: &DatabaseMacroSyntax,
    source: impl Iterator<Item = ast::Item>,
    prefix: &[String],
    inherited_cfgs: &[String],
    out: &mut Vec<SourceProofFunction>,
) {
    for item in source {
        match item {
            ast::Item::Fn(function) => {
                if let Some(name) = function.name() {
                    let name = name.text().to_string();
                    out.push(function_facts(
                        file,
                        &function,
                        prefix,
                        &name,
                        vec![name.clone()],
                        inherited_cfgs,
                        &[],
                    ));
                }
            }
            ast::Item::Module(module) => {
                if let Some(list) = module.item_list()
                    && let Some(name) = module.name()
                {
                    let mut prefix = prefix.to_vec();
                    prefix.push(name.text().to_string());
                    let mut cfgs = inherited_cfgs.to_vec();
                    cfgs.extend(cfg_names(&module));
                    items(file, list.items(), &prefix, &cfgs, out);
                }
            }
            ast::Item::Impl(implementation) => {
                let ty = implementation.self_ty();
                let bare = ty.as_ref().map(type_label).unwrap_or_else(|| "?".into());
                let key = ty
                    .as_ref()
                    .map(native_type_key)
                    .unwrap_or_else(|| "?".into());
                let trait_name = implementation.trait_().as_ref().map(type_label);
                let mut cfgs = inherited_cfgs.to_vec();
                cfgs.extend(cfg_names(&implementation));
                let attrs = attribute_names(&implementation);
                if let Some(list) = implementation.assoc_item_list() {
                    for method in list.assoc_items().filter_map(|item| match item {
                        ast::AssocItem::Fn(method) => Some(method),
                        _ => None,
                    }) {
                        let Some(name) = method.name() else {
                            continue;
                        };
                        let local = match &trait_name {
                            Some(trait_name) => format!("<{key} as {trait_name}>::{name}"),
                            None => format!("{key}::{name}"),
                        };
                        let mut keys = vec![format!("{bare}::{name}")];
                        if let Some(trait_name) = &trait_name {
                            keys.push(format!("{trait_name}::{name}"));
                        }
                        out.push(function_facts(
                            file, &method, prefix, &local, keys, &cfgs, &attrs,
                        ));
                    }
                }
            }
            ast::Item::MacroCall(invocation) => items(
                file,
                file.items(&invocation).iter().cloned(),
                prefix,
                inherited_cfgs,
                out,
            ),
            _ => {}
        }
    }
}

#[tracing::instrument(level = "debug", skip(file, function))]
fn function_facts(
    file: &DatabaseMacroSyntax,
    function: &ast::Fn,
    prefix: &[String],
    local: &str,
    call_keys: Vec<String>,
    inherited_cfgs: &[String],
    inherited_attrs: &[String],
) -> SourceProofFunction {
    let mut cfgs = inherited_cfgs.to_vec();
    cfgs.extend(cfg_names(function));
    let mut attrs = inherited_attrs.to_vec();
    attrs.extend(attribute_names(function));
    let mut calls = Vec::new();
    if let Some(body) = function.body() {
        body_calls(file, body.syntax(), &mut calls);
    }
    SourceProofFunction {
        qualified_name: qualify(prefix, local),
        call_keys,
        cfg_names: cfgs,
        attribute_names: attrs,
        calls,
    }
}

#[tracing::instrument(level = "trace", skip(owner))]
fn cfg_names(owner: &dyn HasAttrs) -> Vec<String> {
    owner
        .attrs()
        .filter_map(|attr| match attr.meta()? {
            ast::Meta::CfgMeta(meta) => match meta.cfg_predicate()? {
                ast::CfgPredicate::CfgAtom(atom) => {
                    atom.ident_token().map(|token| token.text().to_string())
                }
                ast::CfgPredicate::CfgComposite(composite) => {
                    composite.keyword().map(|token| token.text().to_string())
                }
            },
            _ => None,
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(owner))]
fn attribute_names(owner: &dyn HasAttrs) -> Vec<String> {
    owner
        .attrs()
        .filter_map(|attr| {
            let meta = attr.meta()?;
            meta.path()
                .map(|path| path_names(path).into_iter().next())
                .unwrap_or_else(|| meta.simple_name().map(|name| name.to_string()))
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(ty))]
fn type_label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| path.segment())
            .and_then(|seg| seg.name_ref())
            .map(|name| name.text().to_string())
            .unwrap_or_else(|| "?".into()),
        ast::Type::RefType(ty) => ty
            .ty()
            .as_ref()
            .map(type_label)
            .unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(ty) => ty
            .ty()
            .as_ref()
            .map(type_label)
            .unwrap_or_else(|| "?".into()),
        _ => "?".into(),
    }
}

#[tracing::instrument(level = "trace", skip(path))]
fn path_names(path: ast::Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Some(path);
    while let Some(path) = current {
        if let Some(name) = path.segment().and_then(|seg| seg.name_ref()) {
            names.push(name.text().to_string());
        }
        current = path.qualifier();
    }
    names.reverse();
    names
}

#[tracing::instrument(level = "debug", skip(file, body, out))]
fn body_calls(file: &DatabaseMacroSyntax, body: &SyntaxNode, out: &mut Vec<String>) {
    for node in body.descendants() {
        if let Some(call) = ast::CallExpr::cast(node.clone())
            && let Some(ast::Expr::PathExpr(expr)) = call.expr()
            && let Some(path) = expr.path()
        {
            let parts = path_names(path);
            out.push(parts[parts.len().saturating_sub(2)..].join("::"));
        }
        if let Some(invocation) = ast::MacroCall::cast(node)
            && let Some(path) = invocation.path()
        {
            let names = path_names(path);
            let take = match names.as_slice() {
                [name] => assert_arguments(name),
                _ => 0,
            };
            if take > 0 {
                for arg in file.arguments(&invocation).iter().take(take) {
                    body_calls(file, arg.syntax(), out);
                }
            }
        }
    }
}
