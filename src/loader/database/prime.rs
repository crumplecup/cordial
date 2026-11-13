//! Evaluate every local function's evidence on worker threads, ahead of the
//! projection that reads it.
//!
//! The per-function queries are independent, and Salsa runs queries on any
//! handle onto the same storage concurrently, memoising each result once. So the
//! projection, which asks for them one at a time in file order, finds them
//! already computed. A function this pass misses (one nested in a body, say) is
//! evaluated when the projection asks for it, as before: priming changes when a
//! result is computed, never what it is.

use super::{SourceDatabase, queries};
use ra_ap_hir::{AssocItem, Crate, Function, ModuleDef, attach_db};
use ra_ap_ide_db::RootDatabase;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::instrument;

/// Stack for a worker: inferring a body recurses deeply, and a spawned thread's
/// default stack is far smaller than the main thread's.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

impl SourceDatabase {
    /// Evaluate the evidence of every function in the workspace's own crates,
    /// spread over the available cores.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn prime_function_evidence(&self) {
        let functions = attach_db(&self.db, || local_functions(&self.db));
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(functions.len().max(1));
        tracing::debug!(
            target: "cordial::semantic::priming",
            functions = functions.len(),
            workers,
            "priming function evidence"
        );
        let next = AtomicUsize::new(0);
        // A subscriber set for this thread would not see the workers' events.
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let db = self.db.clone();
                let (functions, next, dispatch) = (&functions, &next, dispatch.clone());
                let spawned = std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        tracing::dispatcher::with_default(&dispatch, || {
                            evaluate(&db, functions, next);
                        });
                    });
                if let Err(error) = spawned {
                    // The functions this worker would have taken are evaluated
                    // by the projection instead.
                    tracing::warn!(error = ?error, "could not start a priming worker");
                }
            }
        });
    }
}

/// Take functions from the shared counter until none are left.
#[instrument(level = "debug", skip(db, functions, next))]
#[hotpath::measure]
fn evaluate(db: &RootDatabase, functions: &[Function], next: &AtomicUsize) {
    attach_db(db, || {
        while let Some(function) = functions.get(next.fetch_add(1, Ordering::Relaxed)) {
            let key = queries::FunctionKey::new(db, *function);
            let _ = queries::function_evidence(db, key);
        }
    });
}

/// The functions declared in the workspace's own crates: free functions, trait
/// methods with their default bodies, and the methods of every impl.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn local_functions(db: &RootDatabase) -> Vec<Function> {
    let mut functions = Vec::new();
    for krate in Crate::all(db) {
        if !krate.origin(db).is_local() {
            continue;
        }
        for module in krate.modules(db) {
            for declaration in module.declarations(db) {
                match declaration {
                    ModuleDef::Function(function) => functions.push(function),
                    ModuleDef::Trait(declared) => {
                        functions.extend(declared.items(db).into_iter().filter_map(function_of));
                    }
                    _ => {}
                }
            }
            for implementation in module.impl_defs(db) {
                functions.extend(implementation.items(db).into_iter().filter_map(function_of));
            }
        }
    }
    functions
}

#[instrument(level = "trace")]
fn function_of(item: AssocItem) -> Option<Function> {
    match item {
        AssocItem::Function(function) => Some(function),
        _ => None,
    }
}
