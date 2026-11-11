//! Cordial queries in rust-analyzer's database, including upstream dependencies.

use std::collections::HashSet;

use ra_ap_hir::{Callable, Function, Semantics};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, WalkEvent, ast};
use tracing::instrument;

use crate::{SemanticCall, SemanticCallTarget, SemanticFunction, SemanticReturnContract};

use super::{recording, types};

#[salsa::db]
pub(crate) trait SemanticDatabase: salsa::Database {
    fn root_database(&self) -> &RootDatabase;
}

#[salsa::db]
impl SemanticDatabase for RootDatabase {
    #[instrument(level = "trace", skip(self))]
    fn root_database(&self) -> &RootDatabase {
        self
    }
}

#[salsa::interned]
pub(super) struct FunctionKey<'db> {
    #[returns(copy)]
    function: Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectFacts {
    facts: SemanticFunction,
    source_targets: Vec<Function>,
}

#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
fn direct_facts<'db>(db: &'db dyn SemanticDatabase, key: FunctionKey<'db>) -> Option<DirectFacts> {
    collect_direct(db.root_database(), key.function(db))
}

#[instrument(level = "debug", skip(db, function))]
#[hotpath::measure]
fn collect_direct(db: &RootDatabase, function: Function) -> Option<DirectFacts> {
    let sema = Semantics::new(db);
    let source = sema.source(function)?;
    let label = function.name(db).as_str().to_owned();
    tracing::debug!(target: "cordial::semantic::queries", function = label.as_str(), query = "direct", "semantic query executed");
    let krate = function.module(db).krate(db);
    let output = types::return_contract(&sema, krate, &function.ret_type(db));
    // An attribute macro generated this function. Its body's ranges are expansion
    // coordinates, not the original file's, so its calls stay an explicit gap.
    let expanded = source.file_id.is_macro();
    let mut incomplete = types::incomplete(&output) || source.value.body().is_none() || expanded;
    let mut calls = Vec::new();
    let mut source_targets = Vec::new();
    if let Some(body) = source.value.body().filter(|_| !expanded) {
        let mut walk = BodyWalk {
            sema: &sema,
            krate,
            calls: &mut calls,
            targets: &mut source_targets,
            incomplete: &mut incomplete,
        };
        walk.node(body.syntax(), None);
    }
    Some(DirectFacts {
        facts: SemanticFunction::new(types::identity(db, function.into()), label, output)
            .with_result_recording(recording::result_recording(&sema, function))
            .with_formatting_role(recording::formatting_role(&sema, function))
            .with_return_shape(recording::return_shape(&sema, function))
            .with_return_recording(recording::return_recording(&sema, function))
            .with_parameters(recording::parameters(&sema, function, &source.value))
            .with_calls(calls)
            .with_incomplete(incomplete),
        source_targets,
    })
}

/// The calls in one function body, gathered into the function's facts.
struct BodyWalk<'a, 'db> {
    sema: &'a Semantics<'db, RootDatabase>,
    krate: ra_ap_hir::Crate,
    calls: &'a mut Vec<SemanticCall>,
    targets: &'a mut Vec<Function>,
    incomplete: &'a mut bool,
}

impl BodyWalk<'_, '_> {
    /// Walk `root` for calls. `invocation` is the source range of the macro call
    /// whose expansion `root` is, when it is one: ranges inside an expansion are
    /// not file coordinates, so its calls take the invocation's range.
    #[instrument(level = "debug", skip(self, root, invocation))]
    fn node(
        &mut self,
        root: &ra_ap_syntax::SyntaxNode,
        invocation: Option<ra_ap_syntax::TextRange>,
    ) {
        let mut walk = root.preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            if ast::Fn::can_cast(node.kind()) || ast::ClosureExpr::can_cast(node.kind()) {
                *self.incomplete = true;
                walk.skip_subtree();
                continue;
            }
            if let Some(invoked) = ast::MacroCall::cast(node.clone()) {
                // A macro call hides code from the function's own contract, so the
                // function stays incomplete; what rust-analyzer can expand still
                // contributes its calls to reachability.
                *self.incomplete = true;
                walk.skip_subtree();
                if let Some(expansion) = self.sema.expand_macro_call(&invoked) {
                    let range = invocation.unwrap_or_else(|| invoked.syntax().text_range());
                    self.node(&expansion.value, Some(range));
                }
                continue;
            }
            let Some(call) = ast::Expr::cast(node.clone()).filter(|expr| {
                matches!(expr, ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_))
            }) else {
                continue;
            };
            let (target, output) = call_contract(
                self.sema,
                self.krate,
                semantic_callable(self.sema, &call),
                self.targets,
            );
            *self.incomplete |=
                !matches!(target, SemanticCallTarget::Function(_)) || types::incomplete(&output);
            let range = invocation.unwrap_or_else(|| node.text_range());
            self.calls.push(SemanticCall::new(
                (usize::from(range.start())..usize::from(range.end())).into(),
                target,
                output,
            ));
        }
    }
}

#[instrument(level = "trace", skip(sema, call))]
fn semantic_callable<'db>(
    sema: &Semantics<'db, RootDatabase>,
    call: &ast::Expr,
) -> Option<Callable<'db>> {
    match call {
        ast::Expr::CallExpr(call) => call
            .expr()
            .and_then(|expr| sema.resolve_expr_as_callable(&expr)),
        ast::Expr::MethodCallExpr(call) => sema.resolve_method_call_as_callable(call),
        _ => None,
    }
}

#[instrument(level = "trace", skip(sema, krate, callable, targets))]
fn call_contract(
    sema: &Semantics<'_, RootDatabase>,
    krate: ra_ap_hir::Crate,
    callable: Option<Callable<'_>>,
    targets: &mut Vec<Function>,
) -> (SemanticCallTarget, SemanticReturnContract) {
    let Some(callable) = callable else {
        return (
            SemanticCallTarget::Unresolved,
            SemanticReturnContract::Unavailable,
        );
    };
    let output = types::return_contract(sema, krate, &callable.return_type());
    let target = if let Some(function) = callable.as_function() {
        if function.module(sema.db).krate(sema.db) == krate {
            targets.push(function);
        }
        SemanticCallTarget::Function(types::identity(sema.db, function.into()))
    } else {
        SemanticCallTarget::OtherCallable
    };
    (target, output)
}

#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
pub(super) fn function_evidence<'db>(
    db: &'db dyn SemanticDatabase,
    key: FunctionKey<'db>,
) -> Option<SemanticFunction> {
    collect_reachable(db, key)
}

#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn collect_reachable(db: &dyn SemanticDatabase, key: FunctionKey<'_>) -> Option<SemanticFunction> {
    let facts = direct_facts(db, key).as_ref()?.facts.clone();
    let mut incomplete = *facts.incomplete();
    let mut reachable_result_errors = Vec::new();
    tracing::debug!(target: "cordial::semantic::queries", function = facts.label().as_str(), query = "reachable", "semantic query executed");
    let mut visited = HashSet::new();
    let mut pending = vec![key.function(db)];
    while let Some(function) = pending.pop() {
        if !visited.insert(function) {
            continue;
        }
        let Some(direct) = direct_facts(db, FunctionKey::new(db, function)) else {
            incomplete = true;
            continue;
        };
        incomplete |= *direct.facts.incomplete();
        for output in std::iter::once(direct.facts.output())
            .chain(direct.facts.calls().iter().map(|call| call.output()))
        {
            if let SemanticReturnContract::Result { error, .. } = output
                && !reachable_result_errors.contains(error)
            {
                reachable_result_errors.push(error.clone());
            }
        }
        pending.extend(&direct.source_targets);
    }
    Some(
        facts
            .with_reachable_result_errors(reachable_result_errors)
            .with_incomplete(incomplete),
    )
}
