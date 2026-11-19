//! The identity of what a name in a function body resolves to.
//!
//! Rules ask what a macro, a call or an attribute *is*, not what it is called;
//! the database knows, and these functions read its answer into shared facts.

use ra_ap_hir::{
    AsAssocItem, AssocItemContainer, Function, ItemInNs, ModPath, Name, PathKind, Semantics,
};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, WalkEvent, ast};

use crate::ir::{SemanticCallee, SemanticMacroCall};

/// Every macro invocation in `function`'s body, outside nested functions, with
/// the macro it resolved to. Invocations inside closures count: an error event
/// is often emitted from a `map_err` closure.
#[tracing::instrument(level = "debug", skip(sema, function))]
#[hotpath::measure]
pub(super) fn macro_calls(
    sema: &Semantics<'_, RootDatabase>,
    function: &ast::Fn,
) -> Vec<SemanticMacroCall> {
    let Some(body) = function.body() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut walk = body.syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if ast::Fn::can_cast(node.kind()) {
            walk.skip_subtree();
            continue;
        }
        let Some(invocation) = ast::MacroCall::cast(node) else {
            continue;
        };
        let resolved = sema.resolve_macro_call(&invocation);
        let written = invocation
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
            .unwrap_or_default();
        let call = match resolved {
            Some(definition) => SemanticMacroCall::new(
                definition.name(sema.db).as_str().to_owned(),
                definition
                    .module(sema.db)
                    .krate(sema.db)
                    .display_name(sema.db)
                    .map(|name| name.to_string()),
            ),
            None => SemanticMacroCall::new(written, None),
        };
        tracing::debug!(
            target: "cordial::semantic::identity",
            macro_name = %call.name(),
            krate = ?call.krate(),
            "macro invocation"
        );
        found.push(call);
    }
    found
}

/// Whether `invocation` is the Verus `verus!` macro: the macro its path resolves
/// to is defined by `verus_builtin_macros`, however `vstd` or a rename spells it.
#[cfg(feature = "verus_ir")]
#[tracing::instrument(level = "trace", skip(sema, invocation))]
pub(crate) fn is_verus_macro(
    sema: &Semantics<'_, RootDatabase>,
    invocation: &ast::MacroCall,
) -> bool {
    let resolved = macro_by_path(sema, invocation);
    if resolved
        .as_ref()
        .is_some_and(|found| found.is("verus_builtin_macros", &["verus"]))
    {
        return true;
    }
    if invocation
        .path()
        .is_some_and(|path| path.syntax().text() == "verus")
    {
        tracing::warn!(
            resolved = ?resolved,
            "a macro named verus does not resolve to verus_builtin_macros, so its body is not read; \
             the crate must depend on verus_builtin_macros and import the macro"
        );
    }
    false
}

/// The macro a written macro path names, resolved from the surroundings of the
/// call. This works where resolving the call itself does not: in a function that
/// an attribute macro replaced, the original body is not what the database
/// lowers, but the module around it still scopes the path (imports, the prelude,
/// `crate::` and `$crate`-style paths). A macro defined with textual scope in the
/// same module is not found this way, so it is never mistaken for another.
#[cfg(any(feature = "tracing", feature = "verus_ir"))]
#[tracing::instrument(level = "debug", skip(sema, invocation))]
#[hotpath::measure]
pub(crate) fn macro_by_path(
    sema: &Semantics<'_, RootDatabase>,
    invocation: &ast::MacroCall,
) -> Option<SemanticMacroCall> {
    macro_at(sema, invocation.syntax(), &invocation.path()?)
}

/// The macro `path` names where `scope` is written: the same lookup for an
/// attribute, whose path is resolved from the module around the item it is on.
#[tracing::instrument(level = "debug", skip(sema, scope, path))]
#[hotpath::measure]
pub(crate) fn macro_at(
    sema: &Semantics<'_, RootDatabase>,
    scope: &ra_ap_syntax::SyntaxNode,
    path: &ast::Path,
) -> Option<SemanticMacroCall> {
    let mut kind = PathKind::Plain;
    let mut segments = Vec::new();
    for (index, segment) in path.segments().enumerate() {
        match segment.kind()? {
            ast::PathSegmentKind::Name(name) => segments.push(Name::new_root(name.text())),
            ast::PathSegmentKind::CrateKw => kind = PathKind::Crate,
            ast::PathSegmentKind::SelfKw => kind = PathKind::Super(0),
            ast::PathSegmentKind::SuperKw => {
                kind = match kind {
                    PathKind::Super(depth) => PathKind::Super(depth + 1),
                    _ => PathKind::Super(1),
                }
            }
            _ => return None,
        }
        if index == 0 && segment.coloncolon_token().is_some() {
            kind = PathKind::Abs;
        }
    }
    // Where the path is resolved from: the module `Semantics` finds for the file, which is the first
    // of the crates that hold it when there are several (a cfg context's copies of a crate name
    // the files they do not change as the original's).
    if let Some(around) = sema.scope(scope) {
        tracing::debug!(
            target: "cordial::semantic::identity",
            written = %path.syntax().text(),
            scope_crate = %super::edition::crate_label(sema.db, around.module().krate(sema.db).base()),
            "resolving a macro path from a module"
        );
    }
    let definition = sema
        .resolve_mod_path(scope, &ModPath::from_segments(kind, segments))?
        .find_map(|item| match item {
            ItemInNs::Macros(definition) => Some(definition),
            _ => None,
        })?;
    let found = SemanticMacroCall::new(
        definition.name(sema.db).as_str().to_owned(),
        definition
            .module(sema.db)
            .krate(sema.db)
            .display_name(sema.db)
            .map(|name| name.to_string()),
    );
    tracing::debug!(
        target: "cordial::semantic::identity",
        written = %path.syntax().text(),
        macro_name = %found.name(),
        krate = ?found.krate(),
        "macro path"
    );
    Some(found)
}

/// What the function a call resolved to is: its own name, its crate, and the
/// canonical path of its definition, including the trait or type it belongs to.
#[tracing::instrument(level = "trace", skip(sema, function))]
pub(super) fn callee(sema: &Semantics<'_, RootDatabase>, function: Function) -> SemanticCallee {
    let db = sema.db;
    let module = function.module(db);
    let krate = module
        .krate(db)
        .display_name(db)
        .map(|name| name.to_string());
    let mut parts: Vec<String> = krate.iter().cloned().collect();
    parts.extend(
        module
            .path_to_root(db)
            .into_iter()
            .rev()
            .filter_map(|module| module.name(db))
            .map(|name| name.as_str().to_owned()),
    );
    match function.as_assoc_item(db).map(|item| item.container(db)) {
        Some(AssocItemContainer::Trait(declared)) => {
            parts.push(declared.name(db).as_str().to_owned());
        }
        Some(AssocItemContainer::Impl(implementation)) => {
            let owner = match implementation.trait_(db) {
                Some(declared) => Some(declared.name(db).as_str().to_owned()),
                None => implementation
                    .self_ty(db)
                    .as_adt()
                    .map(|adt| adt.name(db).as_str().to_owned()),
            };
            parts.extend(owner);
        }
        None => {}
    }
    let name = function.name(db).as_str().to_owned();
    parts.push(name.clone());
    SemanticCallee::new(name, krate, parts.join("::"))
}
