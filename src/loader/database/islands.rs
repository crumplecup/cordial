//! Functions inside item-position macro invocations, read from their tokens.
//!
//! rust-analyzer does not see these as definitions of their own when it cannot
//! or does not expand the macro, and the code a verifier's macro emits exists
//! only in that verifier's build. The tokens are parsed as items once, with the
//! database's syntax; each call path is resolved as if written at the
//! invocation, which is a semantic resolution and never a name match.

use ra_ap_hir::{AssocItem, Function, ModuleDef, PathResolution, Semantics, SemanticsScope};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};

use super::types;
use crate::SemanticItemId;
use crate::ir::source_attributes::native_type_key;
use crate::loader::DatabaseMacroSyntax;

/// What reading the invocations of one file needs: the database to resolve in,
/// the file, the syntax of its macro islands, and the file's module path.
#[derive(derive_new::new, derive_getters::Getters)]
#[new(visibility = "pub(super)")]
pub(super) struct Reading<'a, 'db> {
    db: &'a RootDatabase,
    sema: &'a Semantics<'db, RootDatabase>,
    macros: &'a DatabaseMacroSyntax,
    module_path: &'a str,
}

/// One function found in a macro invocation's tokens.
#[derive(derive_new::new, derive_getters::Getters)]
#[new(visibility = "pub(super)")]
pub(super) struct IslandFunction {
    /// The function's path below the file's module: `name`, `module::name`, or
    /// `<Type as Trait>::name` for a method.
    path: String,
    gates: Vec<String>,
    /// Every path the body calls, including calls in the arguments of `assert!`.
    calls: Vec<ast::Path>,
}

/// The functions in `invocation`'s tokens, nested invocations included.
#[tracing::instrument(level = "debug", skip(macros, invocation))]
#[hotpath::measure]
pub(super) fn functions(
    macros: &DatabaseMacroSyntax,
    invocation: &ast::MacroCall,
) -> Vec<IslandFunction> {
    let mut found = Vec::new();
    walk(
        macros,
        macros.items(invocation).iter().cloned(),
        &[],
        &[],
        &mut found,
    );
    found
}

/// The function `path` names when written at `invocation`, if it names one.
#[tracing::instrument(level = "trace", skip(reading, invocation, path))]
pub(super) fn resolve(
    reading: &Reading<'_, '_>,
    invocation: &ast::MacroCall,
    path: &ast::Path,
) -> Option<SemanticItemId> {
    let (sema, db): (&Semantics<'_, RootDatabase>, &RootDatabase) = (reading.sema(), reading.db());
    let scope = sema.scope(invocation.syntax())?;
    let resolution = scope.speculative_resolve(path);
    let function = match &resolution {
        Some(PathResolution::Def(ModuleDef::Function(function))) => Some(*function),
        // `Type::function` and `Trait::function` need the qualifier resolved
        // first: an associated function is not in the path's own namespace.
        _ => associated(&scope, db, path),
    };
    tracing::debug!(
        target: "cordial::semantic::islands",
        path = %path.syntax().text(),
        resolution = ?resolution.as_ref().map(std::mem::discriminant),
        resolved = function.is_some(),
        "island call path"
    );
    function.map(|function| types::identity(db, function.into()))
}

#[tracing::instrument(level = "debug", skip(macros, items, out))]
#[hotpath::measure]
fn walk(
    macros: &DatabaseMacroSyntax,
    items: impl Iterator<Item = ast::Item>,
    prefix: &[String],
    inherited_cfgs: &[String],
    out: &mut Vec<IslandFunction>,
) {
    for item in items {
        match item {
            ast::Item::Fn(function) => {
                if let Some(name) = function.name() {
                    let name = name.text().to_string();
                    out.push(function_facts(
                        macros,
                        &function,
                        qualify(prefix, &name),
                        inherited_cfgs,
                        &[],
                    ));
                }
            }
            ast::Item::Module(module) => {
                if let Some(list) = module.item_list()
                    && let Some(name) = module.name()
                {
                    let mut nested = prefix.to_vec();
                    nested.push(name.text().to_string());
                    let mut cfgs = inherited_cfgs.to_vec();
                    cfgs.extend(cfg_names(&module));
                    walk(macros, list.items(), &nested, &cfgs, out);
                }
            }
            ast::Item::Impl(implementation) => {
                let key = implementation
                    .self_ty()
                    .as_ref()
                    .map(native_type_key)
                    .unwrap_or_else(|| "?".into());
                let trait_name = implementation.trait_().as_ref().map(native_type_key);
                let mut cfgs = inherited_cfgs.to_vec();
                cfgs.extend(cfg_names(&implementation));
                let attrs = attribute_names(&implementation);
                let Some(list) = implementation.assoc_item_list() else {
                    continue;
                };
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
                    out.push(function_facts(
                        macros,
                        &method,
                        qualify(prefix, &local),
                        &cfgs,
                        &attrs,
                    ));
                }
            }
            ast::Item::MacroCall(inner) => walk(
                macros,
                macros.items(&inner).iter().cloned(),
                prefix,
                inherited_cfgs,
                out,
            ),
            _ => {}
        }
    }
}

#[tracing::instrument(level = "debug", skip(macros, function, path))]
#[hotpath::measure]
fn function_facts(
    macros: &DatabaseMacroSyntax,
    function: &ast::Fn,
    path: String,
    inherited_cfgs: &[String],
    inherited_attrs: &[String],
) -> IslandFunction {
    let mut gates = inherited_cfgs.to_vec();
    gates.extend(cfg_names(function));
    gates.extend(inherited_attrs.iter().cloned());
    gates.extend(attribute_names(function));
    let mut calls = Vec::new();
    if let Some(body) = function.body() {
        body_calls(macros, body.syntax(), &mut calls);
    }
    IslandFunction::new(path, gates, calls)
}

#[tracing::instrument(level = "trace")]
fn qualify(prefix: &[String], local: &str) -> String {
    prefix
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(local))
        .collect::<Vec<_>>()
        .join("::")
}

/// The atom of each `#[cfg(atom)]` on `owner`, or the combinator's keyword.
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

/// The first path segment of each attribute on `owner`.
#[tracing::instrument(level = "trace", skip(owner))]
fn attribute_names(owner: &dyn HasAttrs) -> Vec<String> {
    owner
        .attrs()
        .filter_map(|attr| {
            let meta = attr.meta()?;
            meta.path()
                .and_then(first_segment)
                .or_else(|| meta.simple_name().map(|name| name.to_string()))
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(path))]
fn first_segment(path: ast::Path) -> Option<String> {
    let mut current = path;
    while let Some(qualifier) = current.qualifier() {
        current = qualifier;
    }
    current
        .segment()
        .and_then(|segment| segment.name_ref())
        .map(|name| name.text().to_string())
}

/// How many leading arguments of an assertion macro are expressions to read.
#[tracing::instrument(level = "trace")]
fn assert_arguments(name: &str) -> usize {
    match name {
        "assert" | "debug_assert" => 1,
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => 2,
        _ => 0,
    }
}

#[tracing::instrument(level = "debug", skip(macros, body, out))]
#[hotpath::measure]
fn body_calls(
    macros: &DatabaseMacroSyntax,
    body: &ra_ap_syntax::SyntaxNode,
    out: &mut Vec<ast::Path>,
) {
    for node in body.descendants() {
        if let Some(call) = ast::CallExpr::cast(node.clone())
            && let Some(ast::Expr::PathExpr(expr)) = call.expr()
            && let Some(path) = expr.path()
        {
            out.push(path);
        }
        if let Some(invocation) = ast::MacroCall::cast(node)
            && let Some(path) = invocation.path()
            && let Some(name) = path.segment().and_then(|segment| segment.name_ref())
            && path.qualifier().is_none()
        {
            for argument in macros
                .arguments(&invocation)
                .iter()
                .take(assert_arguments(name.text()))
            {
                body_calls(macros, argument.syntax(), out);
            }
        }
    }
}

/// The associated function `Qualifier::name` names, found by resolving the
/// qualifier to a type or a trait and looking `name` up among its candidates.
/// A call through a trait resolves to the trait's declaration, as it does in
/// the body of a real function.
#[tracing::instrument(level = "trace", skip(scope, db, path))]
fn associated(scope: &SemanticsScope<'_>, db: &RootDatabase, path: &ast::Path) -> Option<Function> {
    let qualifier = path.qualifier()?;
    let name = path.segment()?.name_ref()?;
    let named = |function: Function| function.name(db).as_str() == name.text();
    let ty = match scope.speculative_resolve(&qualifier)? {
        PathResolution::Def(ModuleDef::Trait(declared)) => {
            return declared.items(db).into_iter().find_map(|item| match item {
                AssocItem::Function(function) if named(function) => Some(function),
                _ => None,
            });
        }
        PathResolution::Def(ModuleDef::Adt(adt)) => adt.ty(db),
        PathResolution::Def(ModuleDef::TypeAlias(alias)) => alias.ty(db),
        PathResolution::Def(ModuleDef::BuiltinType(builtin)) => builtin.ty(db),
        PathResolution::SelfType(implementation) => implementation.self_ty(db),
        _ => return None,
    };
    ty.iterate_path_candidates(
        db,
        scope,
        &scope.visible_traits(),
        None,
        |item| match item {
            AssocItem::Function(function) if named(function) => Some(function),
            _ => None,
        },
    )
}
