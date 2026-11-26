//! Cordial queries in rust-analyzer's database, including upstream dependencies.

use std::collections::HashSet;

use ra_ap_hir::{Callable, Function, Semantics};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, WalkEvent, ast};
use tracing::instrument;

use crate::ir::SemanticCallee;
use crate::{SemanticCall, SemanticCallTarget, SemanticFunction, SemanticReturnContract};

use super::{identity, recording, types};

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

/// How many function-evidence queries have run to completion in this process, as
/// opposed to being answered from a memo. A pass that wants to say how much of
/// its work was new reads it before and after.
static EVIDENCE_EXECUTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The keys whose saved evidence this process computed: the rest it took from the saved state.
static COMPUTED_THIS_RUN: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<u64>>> =
    std::sync::LazyLock::new(Default::default);

/// Forget what has been computed: a projection starts, and what it computes is what it counts.
#[instrument(level = "trace")]
pub(super) fn forget_computed() {
    if let Ok(mut computed) = COMPUTED_THIS_RUN.lock() {
        computed.clear();
    }
}

/// Whether this process computed the saved evidence of the function with `key`.
#[instrument(level = "trace")]
pub(super) fn computed_this_run(key: u64) -> bool {
    COMPUTED_THIS_RUN
        .lock()
        .is_ok_and(|computed| computed.contains(&key))
}

/// How many functions' saved evidence this process computed.
#[instrument(level = "trace")]
pub(super) fn computed_count() -> usize {
    COMPUTED_THIS_RUN
        .lock()
        .map_or(0, |computed| computed.len())
}

/// The number of function-evidence queries executed so far.
#[instrument(level = "trace")]
pub(super) fn evidence_executed() -> usize {
    EVIDENCE_EXECUTED.load(std::sync::atomic::Ordering::Relaxed)
}

/// A function, as the queries that are saved ask about it. Never reused for another: a saved memo
/// holds it, and a restored process has to find it the same function.
#[salsa::interned(persist, revisions = usize::MAX)]
pub(super) struct FunctionKey<'db> {
    #[returns(copy)]
    function: Function,
}

/// A source file the per-file queries read, by the database's id for it and the
/// path the IR names it by.
#[salsa::interned(persist)]
pub(super) struct FileKey<'db> {
    #[returns(copy)]
    pub(super) file: ra_ap_vfs::FileId,
    #[returns(ref)]
    pub(super) path: std::path::PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
/// What the projection derives from one file alone: its module scope and its
/// attribute evidence. Both are functions of the file's text and what its
/// macros resolve to, so Salsa recomputes them only for a file whose own text,
/// or whose macros' resolution, changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileFacts {
    scope: crate::ir::SourceModuleScope,
    attributes: crate::ir::SourceFileAttributes,
}

impl FileFacts {
    /// The file's conservative namespace constraints.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn scope(&self) -> &crate::ir::SourceModuleScope {
        &self.scope
    }

    /// The file's attribute evidence.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn attributes(&self) -> &crate::ir::SourceFileAttributes {
        &self.attributes
    }
}

/// The file's facts, or `None` for a file that did not parse (the projection
/// has already refused it by then).
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn file_facts<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<FileFacts> {
    collect_file_facts(db.root_database(), key.file(db), key.path(db))
}

#[instrument(level = "debug", skip(db, path))]
#[hotpath::measure]
fn collect_file_facts(
    db: &RootDatabase,
    file: ra_ap_vfs::FileId,
    path: &std::path::Path,
) -> Option<FileFacts> {
    let sema = Semantics::new(db);
    let source = super::projection::source_of(db, &sema, file, path).ok()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %path.display(), query = "file_facts", "semantic query executed");
    Some(FileFacts {
        scope: crate::ir::SourceModuleScope::collect(&source),
        attributes: crate::ir::SourceFileAttributes::collect(&source, &|path, scope| {
            identity::macro_resolution(&sema, scope, path)
        }),
    })
}

/// What one function's body says on its own, without what the functions it calls say: saved with the
/// database, so that a function whose body did not change is not read again when something that
/// calls it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DirectFacts {
    facts: SemanticFunction,
    source_targets: Vec<Function>,
}

#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn direct_facts<'db>(db: &'db dyn SemanticDatabase, key: FunctionKey<'db>) -> Option<DirectFacts> {
    let function = key.function(db);
    let facts = collect_direct(db.root_database(), function);
    if tracing::enabled!(target: "cordial::semantic::reach", tracing::Level::DEBUG) {
        // What this memo depends on, by what it read: when something it should have read has changed and it is not out
        // of date, the thing is not here.
        let read = salsa::current_query_inputs()
            .iter()
            .map(|(name, count)| format!("{name}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!(
            target: "cordial::semantic::reach",
            function = ?function,
            found = facts.is_some(),
            reads = %read,
            read_in_order = %salsa::current_query_inputs_in_order()
                .iter()
                .map(|(name, key)| format!("{name}#{key:x}"))
                .collect::<Vec<_>>()
                .join(" "),
            "the direct facts of a function were computed, and this is what they read"
        );
    }
    facts
}

/// Whether a context is flipped now, so that the reachable facts say each function they go on to read: in the ordinary
/// pass there are hundreds of thousands, and it is the flipped ones that are asked about.
static WALK_IS_FLIPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Say that a context is flipped (or not any more).
pub(super) fn say_the_walk_is_flipped(flipped: bool) {
    WALK_IS_FLIPPED.store(flipped, std::sync::atomic::Ordering::Relaxed);
}

/// The crate a function is in: its name and its number, for the log.
fn crate_of(db: &RootDatabase, function: Function) -> (String, u64) {
    use salsa::plumbing::AsId as _;
    let krate = function.module(db).krate(db);
    (
        krate.display_name(db).map_or_else(|| "?".to_owned(), |name| name.to_string()),
        krate.base().as_id().as_bits(),
    )
}

#[instrument(level = "debug", skip(db, function))]
#[hotpath::measure]
fn collect_direct(db: &RootDatabase, function: Function) -> Option<DirectFacts> {
    // Said before anything is read of it: the ids of what is asked for are what a query stack names, and a function that
    // an earlier cfg made can fail to be read at all.
    if tracing::enabled!(target: "cordial::semantic::reach", tracing::Level::DEBUG) {
        let (krate, krate_id) = crate_of(db, function);
        tracing::debug!(
            target: "cordial::semantic::reach",
            function = ?function,
            krate = %krate,
            krate_id = format_args!("{krate_id:#x}"),
            "the direct facts of a function are asked for"
        );
    }
    // A saved memo can hold a function that an edit removed or renamed, and verifying what read it
    // asks again: it has no facts, as one that was never there has none.
    // Nor does one that only a cfg declared, when that cfg is not set: its id still holds its module.
    if !function.is_written(db) || !function.declaration(db).is_declared() {
        return None;
    }
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
    if let Some(body) = source.value.body() {
        let mut walk = BodyWalk {
            sema: &sema,
            krate,
            calls: &mut calls,
            targets: &mut source_targets,
            incomplete: &mut incomplete,
            expansion: expanded,
            anchor: usize::from(source.value.syntax().text_range().start()),
        };
        walk.node(body.syntax(), None);
    }
    if tracing::enabled!(target: "cordial::semantic::reach", tracing::Level::DEBUG) {
        // The functions of its own crate that its body calls, which are what the reachable facts go on to read: where a
        // function enters the walk is here.
        let (krate_name, krate_id) = crate_of(db, function);
        tracing::debug!(
            target: "cordial::semantic::reach",
            function = ?function,
            label = %label,
            krate = %krate_name,
            krate_id = format_args!("{krate_id:#x}"),
            made_by_a_macro = expanded,
            targets = source_targets.len() as u64,
            target_ids = %source_targets.iter().map(|target| format!("{target:?}")).collect::<Vec<_>>().join(" "),
            "the same-crate functions a function calls"
        );
    }
    Some(DirectFacts {
        facts: SemanticFunction::new(types::identity(db, function.into()), label, output)
            .with_result_recording(recording::result_recording(&sema, function))
            .with_formatting_role(recording::formatting_role(&sema, function))
            .with_macro_calls(identity::macro_calls(&sema, &source.value))
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
    /// The body is a macro's expansion, so no range in it is a file coordinate.
    expansion: bool,
    /// Where the function starts in its file: the ranges are recorded relative to it, so text
    /// written above the function leaves them (and the function's evidence) as they were.
    anchor: usize,
}

/// The range of a call inside a macro-generated function. It is no coordinate of
/// any file, so it can never equal the range of a call written in the source.
const EXPANSION_RANGE: usize = usize::MAX;

impl BodyWalk<'_, '_> {
    /// The range a call is recorded under: its own, the invocation's when it came
    /// from a macro call's expansion, or the expansion sentinel when the whole
    /// function was generated.
    #[instrument(level = "trace", skip(self))]
    fn range_of(
        &self,
        own: ra_ap_syntax::TextRange,
        invocation: Option<ra_ap_syntax::TextRange>,
    ) -> (usize, usize) {
        if self.expansion {
            return (EXPANSION_RANGE, EXPANSION_RANGE);
        }
        let range = invocation.unwrap_or(own);
        (
            usize::from(range.start()).saturating_sub(self.anchor),
            usize::from(range.end()).saturating_sub(self.anchor),
        )
    }

    /// Walk `root` for calls. `invocation` is the source range of the macro call
    /// whose expansion `root` is, when it is one: ranges inside an expansion are
    /// not file coordinates, so its calls take the invocation's range.
    #[instrument(level = "debug", skip(self, root, invocation))]
    #[hotpath::measure]
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
            if ast::Fn::can_cast(node.kind()) {
                *self.incomplete = true;
                walk.skip_subtree();
                continue;
            }
            // A closure's calls are calls of this function, but nothing may edit
            // them as if they ran in its own flow.
            if ast::ClosureExpr::can_cast(node.kind()) {
                *self.incomplete = true;
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
            let (target, output, callee) = call_contract(
                self.sema,
                self.krate,
                semantic_callable(self.sema, &call),
                self.targets,
            );
            *self.incomplete |=
                !matches!(target, SemanticCallTarget::Function(_)) || types::incomplete(&output);
            let (start, end) = self.range_of(node.text_range(), invocation);
            self.calls
                .push(SemanticCall::new((start..end).into(), target, output).with_callee(callee));
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
) -> (
    SemanticCallTarget,
    SemanticReturnContract,
    Option<SemanticCallee>,
) {
    let Some(callable) = callable else {
        return (
            SemanticCallTarget::Unresolved,
            SemanticReturnContract::Unavailable,
            None,
        );
    };
    let output = types::return_contract(sema, krate, &callable.return_type());
    let (target, callee) = if let Some(function) = callable.as_function() {
        if function.module(sema.db).krate(sema.db) == krate {
            targets.push(function);
        }
        (
            SemanticCallTarget::Function(types::identity(sema.db, function.into())),
            Some(identity::callee(sema, function)),
        )
    } else {
        (SemanticCallTarget::OtherCallable, None)
    };
    (target, output, callee)
}

#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn function_evidence<'db>(
    db: &'db dyn SemanticDatabase,
    key: FunctionKey<'db>,
) -> Option<SemanticFunction> {
    if tracing::enabled!(target: "cordial::semantic::reach", tracing::Level::DEBUG) {
        tracing::debug!(
            target: "cordial::semantic::reach",
            function = ?key.function(db),
            "the evidence of a function is asked for"
        );
    }
    collect_reachable(db, key)
}

/// The evidence of a function the way the saved database can keep it: keyed by the
/// function's description, which means the same in every process. Computed by the
/// process-local query, and valid for as long as nothing it read has changed.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn persisted_evidence<'db>(
    db: &'db dyn SemanticDatabase,
    key: super::item_key::ItemKey<'db>,
) -> Option<SemanticFunction> {
    tracing::debug!(
        target: "cordial::keys",
        key = super::item_key::key_bits(key),
        description = %super::item_key::key_text(db.root_database(), key),
        "persisted evidence computed"
    );
    if let Ok(mut computed) = COMPUTED_THIS_RUN.lock() {
        computed.insert(super::item_key::key_bits(key));
    }
    // A run in processes that come and go keeps what it computes in a store, by the description,
    // and a process finds there what an earlier one computed.
    let store = crate::result_store::current();
    let description = store
        .as_ref()
        .map(|_| super::item_key::key_text(db.root_database(), key));
    if let (Some(store), Some(description)) = (&store, &description)
        && let Some(bytes) = store.get(crate::result_store::EVIDENCE, description)
        && let Some(kept) = super::item_key::evidence_from_stored(db.root_database(), &bytes)
    {
        tracing::trace!(
            target: "cordial::results",
            description = %description,
            "the evidence was found in the store"
        );
        return kept;
    }
    let evidence = super::item_key::function_of(db, key)
        .and_then(|function| function_evidence(db, FunctionKey::new(db, function)).clone());
    if let (Some(store), Some(description)) = (&store, &description) {
        match super::item_key::evidence_to_stored(db.root_database(), &evidence) {
            Some(bytes) => store.put(crate::result_store::EVIDENCE, description, &bytes),
            None => tracing::warn!(
                target: "cordial::results",
                description = %description,
                "the evidence could not be kept in the store"
            ),
        }
    }
    evidence
}

/// `facts`, the evidence of `function`, with the ranges of its calls as coordinates of its file.
/// The evidence is kept with them relative to where the function starts.
#[instrument(level = "trace", skip(db, facts))]
pub(super) fn in_file(
    db: &RootDatabase,
    function: Function,
    facts: &SemanticFunction,
) -> SemanticFunction {
    match Semantics::new(db).source(function) {
        Some(source) if !source.file_id.is_macro() => {
            facts.with_ranges_from(usize::from(source.value.syntax().text_range().start()))
        }
        _ => facts.clone(),
    }
}

/// The evidence of `function`. From the persisted query when its description finds this
/// same function again (so a restored database can answer without computing it), from the
/// process-local one when it cannot (a function a macro generated, say).
#[instrument(level = "trace", skip(db, function))]
pub(super) fn evidence_of<'db>(
    db: &'db dyn SemanticDatabase,
    function: Function,
) -> &'db Option<SemanticFunction> {
    let root = db.root_database();
    let definition = ra_ap_hir::ModuleDef::Function(function);
    let key = super::item_key::key_of(root, definition);
    if super::item_key::finds(db, key, definition) {
        persisted_evidence(db, key)
    } else {
        tracing::debug!(
            target: "cordial::semantic::identity",
            description = %super::item_key::key_text(root, key),
            "evidence computed in this process: the description does not find this function again"
        );
        function_evidence(db, FunctionKey::new(db, function))
    }
}

#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn collect_reachable(db: &dyn SemanticDatabase, key: FunctionKey<'_>) -> Option<SemanticFunction> {
    EVIDENCE_EXECUTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let facts = direct_facts(db, key).as_ref()?.facts.clone();
    let mut incomplete = *facts.incomplete();
    let mut reachable_result_errors = Vec::new();
    tracing::debug!(target: "cordial::semantic::queries", function = facts.label().as_str(), query = "reachable", "semantic query executed");
    let mut visited = HashSet::new();
    let mut pending = vec![key.function(db)];
    let mut came_from: std::collections::HashMap<Function, Function> = std::collections::HashMap::new();
    while let Some(function) = pending.pop() {
        if !visited.insert(function) {
            continue;
        }
        // Said before the facts of the function are asked for, which verifies what a saved memo of it read: the
        // function is named here by the id that a query stack names it by, and by the function that called it.
        if WALK_IS_FLIPPED.load(std::sync::atomic::Ordering::Relaxed)
            && tracing::enabled!(target: "cordial::semantic::reach", tracing::Level::DEBUG)
        {
            let (krate, krate_id) = crate_of(db.root_database(), function);
            tracing::debug!(
                target: "cordial::semantic::reach",
                root = ?key.function(db),
                function = ?function,
                called_by = ?came_from.get(&function),
                krate = %krate,
                krate_id = format_args!("{krate_id:#x}"),
                "a function the reachable facts of a function go on to read"
            );
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
        for target in &direct.source_targets {
            came_from.entry(*target).or_insert(function);
        }
        pending.extend(&direct.source_targets);
    }
    Some(
        facts
            .with_reachable_result_errors(reachable_result_errors)
            .with_incomplete(incomplete),
    )
}
