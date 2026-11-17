//! Evaluate every local function's evidence on worker threads, ahead of the
//! projection that reads it.
//!
//! The per-function queries are independent, and Salsa runs queries on any
//! handle onto the same storage concurrently, memoising each result once. So the
//! projection, which asks for them one at a time in file order, finds them
//! already computed. A function this pass misses (one nested in a body, say) is
//! evaluated when the projection asks for it, as before: priming changes when a
//! result is computed, never what it is.

use super::item_key::{self, ItemKey};
use super::queries::SemanticDatabase;
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
    pub(super) fn prime_function_evidence(&mut self) {
        // The functions of each ordinary crate, by description, as the last run saved them. A crate
        // all of whose functions a description finds again is primed from them without asking
        // rust-analyzer which functions it has; any other is enumerated.
        let mut described: Vec<String> = Vec::new();
        let complete = attach_db(&self.db, || {
            for krate in self.ordinary_crates() {
                let Some(scope) = item_key::crate_scope(&self.db, Crate::from(krate)) else {
                    tracing::info!(
                        target: "cordial::semantic::fallback",
                        step = "prime",
                        "a crate has no scope of its own: its functions are enumerated through rust-analyzer"
                    );
                    return false;
                };
                let index = crate_functions(&self.db, scope);
                if index.unfound > 0 {
                    tracing::info!(
                        target: "cordial::semantic::fallback",
                        step = "prime",
                        crate_scope = %item_key::key_text(&self.db, scope),
                        unfound = index.unfound,
                        found = index.found.len(),
                        "functions a description cannot find again: every function is enumerated through rust-analyzer"
                    );
                    return false;
                }
                described.extend(index.found.iter().cloned());
            }
            true
        });
        if complete {
            tracing::debug!(
                target: "cordial::semantic::priming",
                functions = described.len(),
                "priming from the saved descriptions of the workspace's functions"
            );
            self.prime_in_chunks("workspace", &described, |db, text| {
                let _ = queries::persisted_evidence(db, ItemKey::new(db, text.clone()));
            });
            return;
        }
        let excluded = self.shadow_crates();
        let functions = attach_db(&self.db, || local_functions(&self.db, &excluded));
        self.prime_in_chunks("workspace", &functions, |db, function| {
            let _ = queries::evidence_of(db, *function);
        });
    }

    #[cfg(feature = "tracing")]
    /// Evaluate the evidence of `functions` on worker threads. The results are
    /// those of the database's cfg as it is now: after the cfg changes they are
    /// computed again, so a pass that enters a context primes inside it.
    ///
    /// Says, as a run event, how many of the functions were evaluated now and how
    /// many were already known, so a pass that recomputes everything shows it.
    #[instrument(level = "debug", skip(self, functions))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn prime_functions(&self, label: &str, functions: &[Function]) {
        self.prime(label, functions, |db, function| {
            let _ = queries::evidence_of(db, *function);
        });
    }

    /// Run `one` over `items` on worker threads, each item once.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self, functions, one))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn prime<T: Sync>(
        &self,
        label: &str,
        functions: &[T],
        one: impl Fn(&RootDatabase, &T) + Sync,
    ) {
        let before = queries::evidence_executed();
        self.prime_part(functions, &one);
        self.say_primed(label, before, functions.len());
    }

    /// [`Self::prime`] in chunks, with what is over its capacity evicted between them. The
    /// workers hold handles onto the database while they run, and eviction needs it exclusively,
    /// so a pass that never stops keeps every syntax tree it makes until it is over.
    #[instrument(level = "debug", skip(self, functions, one))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn prime_in_chunks<T: Sync>(
        &mut self,
        label: &str,
        functions: &[T],
        one: impl Fn(&RootDatabase, &T) + Sync,
    ) {
        let before = queries::evidence_executed();
        let size = chunk_size(functions.len());
        let chunks = functions.len().div_ceil(size);
        for (index, part) in functions.chunks(size).enumerate() {
            crate::peak_heap::note_stage(format!(
                "priming {label}, chunk {} of {chunks}",
                index + 1
            ));
            self.prime_part(part, &one);
            self.evict_between_chunks();
        }
        self.say_primed(label, before, functions.len());
    }

    fn say_primed(&self, label: &str, before: usize, functions: usize) {
        let evaluated = queries::evidence_executed().saturating_sub(before);
        crate::host::event(format!(
            "function evidence ({label}): {evaluated} evaluated for {functions} function(s), the rest already known"
        ));
    }

    /// Run `one` over `functions` on worker threads, each once, and wait for them.
    fn prime_part<T: Sync>(&self, functions: &[T], one: &(impl Fn(&RootDatabase, &T) + Sync)) {
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
            for index in 0..workers {
                let db = self.db.clone();
                let (functions, next, dispatch) = (functions, &next, dispatch.clone());
                let spawned = std::thread::Builder::new()
                    .name(format!("cordial-prime-{index}"))
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        tracing::dispatcher::with_default(&dispatch, || {
                            evaluate(&db, functions, next, one);
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

/// How many functions go between two evictions: enough that the workers' start-up is nothing
/// to it, few enough that what a chunk parses is a small part of the whole.
fn chunk_size(functions: usize) -> usize {
    const CHUNKS: usize = 16;
    const LEAST: usize = 64;
    functions.div_ceil(CHUNKS).max(LEAST)
}

/// Take functions from the shared counter until none are left.
#[instrument(level = "debug", skip(db, functions, next, one))]
#[hotpath::measure]
fn evaluate<T>(
    db: &RootDatabase,
    functions: &[T],
    next: &AtomicUsize,
    one: &impl Fn(&RootDatabase, &T),
) {
    attach_db(db, || {
        while let Some(function) = functions.get(next.fetch_add(1, Ordering::Relaxed)) {
            one(db, function);
        }
    });
}

/// The functions declared in the workspace's own crates: free functions, trait
/// methods with their default bodies, and the methods of every impl.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn local_functions(
    db: &RootDatabase,
    excluded: &std::collections::BTreeSet<ra_ap_ide_db::base_db::Crate>,
) -> Vec<Function> {
    let mut functions = Vec::new();
    for krate in Crate::all(db) {
        if !krate.origin(db).is_local() || excluded.contains(&krate.base()) {
            continue;
        }
        functions.extend(functions_in(db, krate));
    }
    functions
}

/// The functions `krate` declares.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn functions_in(db: &RootDatabase, krate: Crate) -> Vec<Function> {
    let mut functions = Vec::new();
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
    functions
}

/// The descriptions of the functions of one crate, and how many it declares that a description
/// cannot find again.
///
/// Saved with the database, so a run that changed nothing the crate's definitions read knows which
/// functions the crate has without asking rust-analyzer: that would build the definition maps,
/// which are not saved. Each function's evidence is found by its description alone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct CrateFunctions {
    pub(super) found: Vec<String>,
    pub(super) unfound: usize,
}

/// The functions of the crate `scope` describes: see [`CrateFunctions`].
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, scope))]
#[hotpath::measure]
pub(super) fn crate_functions<'db>(
    db: &'db dyn SemanticDatabase,
    scope: ItemKey<'db>,
) -> CrateFunctions {
    let root = db.root_database();
    attach_db(root, || {
        let mut index = CrateFunctions {
            found: Vec::new(),
            unfound: 0,
        };
        let Some(krate) = item_key::crate_of(root, &item_key::key_text(root, scope)) else {
            index.unfound = 1;
            return index;
        };
        for function in functions_in(root, krate) {
            let definition = ModuleDef::Function(function);
            let key = item_key::key_of(root, definition);
            if item_key::finds(db, key, definition) {
                index.found.push(item_key::key_text(root, key));
            } else {
                index.unfound += 1;
            }
        }
        index
    })
}

#[instrument(level = "trace")]
fn function_of(item: AssocItem) -> Option<Function> {
    match item {
        AssocItem::Function(function) => Some(function),
        _ => None,
    }
}
