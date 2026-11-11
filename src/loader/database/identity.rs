//! The identity of what a name in a function body resolves to.
//!
//! Rules ask what a macro, a call or an attribute *is*, not what it is called;
//! the database knows, and these functions read its answer into shared facts.

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, WalkEvent, ast};

use crate::ir::SemanticMacroCall;

/// Every macro invocation in `function`'s body, outside nested functions, with
/// the macro it resolved to. Invocations inside closures count: an error event
/// is often emitted from a `map_err` closure.
#[tracing::instrument(level = "debug", skip(sema, function))]
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
