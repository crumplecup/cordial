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
    let mut incomplete = types::incomplete(&output) || source.value.body().is_none();
    let mut calls = Vec::new();
    let mut source_targets = Vec::new();
    if let Some(body) = source.value.body() {
        let mut walk = body.syntax().preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            if ast::Fn::can_cast(node.kind())
                || ast::ClosureExpr::can_cast(node.kind())
                || ast::MacroCall::can_cast(node.kind())
            {
                incomplete = true;
                walk.skip_subtree();
                continue;
            }
            let callable = if let Some(call) = ast::CallExpr::cast(node.clone()) {
                call.expr()
                    .and_then(|expr| sema.resolve_expr_as_callable(&expr))
            } else if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                sema.resolve_method_call_as_callable(&call)
            } else {
                continue;
            };
            let (target, output) = call_contract(&sema, krate, callable, &mut source_targets);
            incomplete |=
                !matches!(target, SemanticCallTarget::Function(_)) || types::incomplete(&output);
            let range = node.text_range();
            calls.push(SemanticCall::new(
                (usize::from(range.start())..usize::from(range.end())).into(),
                target,
                output,
            ));
        }
    }
    Some(DirectFacts {
        facts: SemanticFunction::new(types::identity(db, function.into()), label, output)
            .with_result_recording(recording::result_recording(&sema, function))
            .with_formatting_role(recording::formatting_role(&sema, function))
            .with_calls(calls)
            .with_incomplete(incomplete),
        source_targets,
    })
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
            if let SemanticReturnContract::Result { error, .. } = output {
                if !reachable_result_errors.contains(error) {
                    reachable_result_errors.push(error.clone());
                }
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
