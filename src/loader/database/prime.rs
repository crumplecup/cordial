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
use std::sync::Arc;
use tracing::instrument;

impl SourceDatabase {
    /// Evaluate the evidence of every function in the workspace's own crates,
    /// spread over the available cores.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn prime_function_evidence(&mut self) -> bool {
        self.build_definition_maps();
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
                // What the process held, and what the machine saw of it, once this crate's functions
                // were found: finding them asks the crate for its files, which builds its definition
                // map and those of what it depends on, and the growth from one crate to the next says
                // where the syntax trees and expansions come from.
                tracing::info!(
                    target: "cordial::enumeration",
                    krate = %item_key::key_text(&self.db, scope),
                    functions = index.found.len(),
                    held_kib = crate::memory_limit::meter_kib().unwrap_or(0),
                    resident_kib = crate::memory_sample().map_or(0, |memory| memory.resident_kib()),
                    "a crate's functions were found"
                );
            }
            true
        });
        if complete {
            tracing::debug!(
                target: "cordial::semantic::priming",
                functions = described.len(),
                "priming from the saved descriptions of the workspace's functions"
            );
            // What an earlier process of this run did is in the store, and is not done again.
            let done = crate::result_store::current()
                .map(|store| store.names(crate::result_store::EVIDENCE))
                .unwrap_or_default();
            let pending: Vec<String> = described
                .into_iter()
                .filter(|text| !done.contains(text))
                .collect();
            tracing::info!(
                target: "cordial::results",
                done = done.len(),
                pending = pending.len(),
                "functions of the workspace whose evidence is in the store, and whose is not"
            );
            let had_work = !pending.is_empty();
            if had_work {
                self.check_room();
            }
            self.prime_in_chunks("workspace", &pending, |db, text| {
                let _ = queries::persisted_evidence(db, ItemKey::new(db, text.clone()));
            });
            return had_work;
        }
        let excluded = self.shadow_crates();
        let mut functions = attach_db(&self.db, || local_functions(&self.db, &excluded));
        if let Some(store) = crate::result_store::current() {
            // What an earlier process of this run did is in the store, and is not done again.
            let done = store.names(crate::result_store::EVIDENCE);
            let before = functions.len();
            attach_db(&self.db, || {
                functions.retain(|function| {
                    let key = item_key::key_of(&self.db, ra_ap_hir::ModuleDef::Function(*function));
                    !done.contains(&item_key::key_text(&self.db, key))
                });
            });
            tracing::info!(
                target: "cordial::results",
                done = done.len(),
                pending = functions.len(),
                enumerated = before,
                "functions of the workspace whose evidence is in the store, and whose is not"
            );
        }
        let had_work = !functions.is_empty();
        if had_work {
            self.check_room();
        }
        self.prime_in_chunks("workspace", &functions, |db, function| {
            let _ = queries::evidence_of(db, *function);
        });
        had_work
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

    /// What this process holds now that the workspace is loaded and its functions are listed is its
    /// floor. If that leaves no room to work in before the process would end, no process of the run
    /// can do more than load, and the run is not one that processes finish within this budget: it
    /// says so now, and ends, and not after as many processes as it takes to find that out.
    fn check_room(&mut self) {
        if !crate::memory_limit::resumable() || !crate::result_store::active() {
            return;
        }
        // The scaffolding that loading and listing built is let go first, if the run is pressed: what
        // a process has to hold to work is what is left.
        self.jettison_scaffolding("floor");
        let Some(memory) = crate::memory_sample() else {
            return;
        };
        // What the machine sees of the process does not fall when the program frees: the allocator
        // keeps it, and does not give it back to what the program allocates next as well as the
        // program would hope. The line a process ends at is judged by what the machine sees, so the
        // floor is no less than that, and no less than what the program holds with the allowance the
        // allocator needs on top of it.
        let held_with_allowance = crate::memory_limit::meter_kib().map_or(0, |held| {
            held.saturating_mul(100 + crate::memory_limit::ALLOCATOR_OVERHEAD_PERCENT) / 100
        });
        let floor = memory.resident_kib().max(held_with_allowance);
        crate::memory_limit::note_floor(floor);
        if crate::no_room(floor, crate::memory_limit::ladder_bytes()) {
            tracing::info!(
                target: "cordial::eviction",
                floor_kib = floor,
                line_kib = crate::checkpoint_line_bytes(crate::memory_limit::ladder_bytes()) / 1024,
                "a process has no room to work in once the workspace is loaded"
            );
            crate::memory_limit::no_room_exit(floor);
        }
    }

    /// Run `one` over `items` on worker threads, each item once.
    #[cfg(feature = "tracing")]
    #[instrument(level = "debug", skip(self, functions, one))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn prime<T: Clone + Send + Sync + 'static>(
        &self,
        label: &str,
        functions: &[T],
        one: impl Fn(&RootDatabase, &T) + Send + Sync + 'static,
    ) {
        let before = queries::evidence_executed();
        let items: Arc<[T]> = functions.to_vec().into();
        let one: Work<T> = Arc::new(one);
        self.prime_round(&items, 0..items.len(), &one, false);
        self.say_primed(label, before, functions.len());
    }

    /// [`Self::prime`] in chunks, with what is over its capacity evicted between them. The
    /// workers hold handles onto the database while they run, and eviction needs it exclusively,
    /// so a pass that never stops keeps every syntax tree it makes until it is over.
    #[instrument(level = "debug", skip(self, functions, one))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn prime_in_chunks<T: Clone + Send + Sync + 'static>(
        &mut self,
        label: &str,
        functions: &[T],
        one: impl Fn(&RootDatabase, &T) + Send + Sync + 'static,
    ) {
        let before = queries::evidence_executed();
        // The evidence is what a run in processes keeps in its store: here a process may end.
        self.run_in_chunks_with(label, functions, one, true);
        self.say_primed(label, before, functions.len());
    }

    /// Run `one` over `items` on worker threads in chunks, evicting between the chunks if asked to.
    /// `stores_results` says whether what `one` computes is kept in the store of a run in processes:
    /// only then may the process end between the chunks, since what it did is not lost.
    pub(in crate::loader::database) fn run_in_chunks_with<T: Clone + Send + Sync + 'static>(
        &mut self,
        label: &str,
        items: &[T],
        one: impl Fn(&RootDatabase, &T) + Send + Sync + 'static,
        stores_results: bool,
    ) {
        let items: Arc<[T]> = items.to_vec().into();
        let one: Work<T> = Arc::new(one);
        let usual = chunk_size(items.len());
        let mut size = usual;
        let mut at = 0;
        let mut index = 0;
        // What a function of this pass has retained, as the last chunk showed.
        let mut growth = crate::FIRST_GROWTH_KIB;
        while at < items.len() {
            // The ladder acts between chunks, so a chunk is as long as what the headroom allows at
            // the growth seen: a chunk that grows past it cannot be stopped.
            let taken = crate::memory_limit::headroom_kib().map_or(size, |headroom| {
                size.min(crate::functions_within(headroom, growth, size))
            });
            let end = (at + taken).min(items.len());
            index += 1;
            let held_before = crate::memory_limit::meter_kib();
            let chunk_started = std::time::Instant::now();
            // Under pressure the workers stop early and the rest of the chunk waits for the next.
            let done = match self.prime_round(&items, at..end, &one, true) {
                // No worker started: the projection evaluates what was not, as it says it will.
                0 => end - at,
                done => done,
            };
            let micros = u64::try_from(chunk_started.elapsed().as_micros()).unwrap_or(u64::MAX);
            let grown_kib = match (held_before, crate::memory_limit::meter_kib()) {
                (Some(before), Some(after)) => {
                    let observed =
                        after.saturating_sub(before) / u64::try_from(done).unwrap_or(1).max(1);
                    // What was seen, but not less than half of what was before: one cheap chunk is
                    // not a reason to take a chunk that is not.
                    growth = observed.max(growth / 2).max(1);
                    after.saturating_sub(before)
                }
                _ => 0,
            };
            at += done;
            if stores_results {
                crate::memory_limit::note_remaining(u64::try_from(items.len() - at).unwrap_or(0));
            }
            let evicted = self.evict_between_chunks(stores_results);
            // What a chunk took, what it took to do it, and what is resident once eviction has
            // given back what it could: the growth of a pass is what stays.
            tracing::debug!(
                target: "cordial::prime",
                label,
                chunk = index,
                files = end - at + done,
                done,
                remaining = items.len() - at,
                grown_kib,
                micros,
                resident_after_kib = crate::memory_limit::meter_kib().unwrap_or(0),
                evicted = u64::from(evicted),
                "priming a chunk"
            );
            size = if evicted {
                (size / 2).max(least_at(crate::memory_limit::pressure()))
            } else {
                // Relieved: back to the usual step, doubling so that one calm sample does not.
                (size * 2).min(usual)
            };
        }
    }

    fn say_primed(&self, label: &str, before: usize, functions: usize) {
        let evaluated = queries::evidence_executed().saturating_sub(before);
        crate::host::event(format!(
            "function evidence ({label}): {evaluated} evaluated for {functions} function(s), the rest already known"
        ));
    }

    /// Run `one` over `items[range]` on the pool, each once, and wait for it. Returns how many it
    /// ran. All of them, unless `may_stop` and the run is under memory pressure, when the workers
    /// take no new function once [`LEAST_UNDER_PRESSURE`] are taken, so that the caller can evict
    /// before the rest.
    ///
    /// How many workers take part is what fits below the second rung, decided before the round
    /// starts: reacting when the rungs are crossed is too late, with every worker already in the
    /// middle of a function. Each carries its own handle onto the database and drops it when the
    /// round is over, so that the caller can take the database exclusively between rounds.
    fn prime_round<T: Send + Sync + 'static>(
        &self,
        items: &Arc<[T]>,
        range: std::ops::Range<usize>,
        one: &Work<T>,
        may_stop: bool,
    ) -> usize {
        let available = crate::pool_size();
        let fitting = crate::memory_limit::headroom_kib().map_or(available, |headroom| {
            crate::workers_within(headroom, available)
        });
        let length = range.len();
        let workers = crate::workers_at(available, crate::memory_limit::pressure())
            .min(fitting)
            .min(length.max(1));
        tracing::debug!(
            target: "cordial::semantic::priming",
            functions = length,
            workers,
            "priming function evidence"
        );
        // The gate says how many items run at once: what fits now, and the ladder narrows or widens
        // it while the round runs. Every item that runs is lent a handle onto the database, which
        // is dropped with the round, so that the caller can take the database exclusively after.
        crate::allow_workers(workers);
        let handles: Vec<RootDatabase> = (0..available.min(length.max(1)))
            .map(|_| self.db.clone())
            .collect();
        let one = Arc::clone(one);
        crate::run_items(
            items,
            range,
            handles,
            move |db, item| prime_item(db, item, &*one),
            move |taken| {
                may_stop
                    && taken >= least_at(crate::memory_limit::pressure())
                    && crate::memory_limit::pressured()
            },
        )
    }
}

/// What a worker runs for each item.
type Work<T> = Arc<dyn Fn(&RootDatabase, &T) + Send + Sync>;

/// The fewest functions a chunk holds when the process is nearing its memory limit.
const LEAST_UNDER_PRESSURE: usize = 8;

/// The fewest on the last rung, where one function at a time is what there is room for.
const LEAST_WHEN_SEVERE: usize = 1;

/// The fewest functions a chunk holds on the rung of the pressure ladder the run is on.
fn least_at(level: crate::Pressure) -> usize {
    if level >= crate::Pressure::Severe {
        LEAST_WHEN_SEVERE
    } else {
        LEAST_UNDER_PRESSURE
    }
}

/// Run `one` on an item with the database attached. A span of its own, named `evaluate` as the
/// workers' loop was, so that the queries it executes are told apart from those of the steps that
/// ask for the priming.
#[instrument(level = "debug", name = "evaluate", skip_all)]
fn prime_item<T>(db: &RootDatabase, item: &T, one: &(dyn Fn(&RootDatabase, &T) + Send + Sync)) {
    attach_db(db, || one(db, item));
}

/// How many functions go between two evictions: enough that the workers' start-up is nothing
/// to it, few enough that what a chunk parses is a small part of the whole.
fn chunk_size(functions: usize) -> usize {
    const CHUNKS: usize = 16;
    const LEAST: usize = 64;
    functions.div_ceil(CHUNKS).max(LEAST)
}

impl SourceDatabase {
    /// Build the definition map of every crate the ordinary crates need, dependencies first, one
    /// crate at a time, and under pressure let go of the scaffolding (trees, expansions, span maps)
    /// that building them left whenever enough of it has built up.
    ///
    /// Asking for a crate's definition map builds those of everything it depends on, inside one query
    /// that nothing can interrupt, and what each needs is parsed and expanded on the way: for a
    /// workspace of modest size that is two thirds of what a process holds once it has loaded. Built
    /// in dependency order, each crate finds what it needs already built, builds only its own, and
    /// the scaffolding of those before it is let go of between them. The maps are the same ones the
    /// first ask would have built, and are all needed, so no work is added.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn build_definition_maps(&mut self) {
        // Which call this is: the projection asks twice, and a crate whose map is built again by a
        // later call is something to see, so each call's events are named by it.
        static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let call = ordinal(CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let needed: std::collections::BTreeSet<ra_ap_ide_db::base_db::Crate> =
            attach_db(&self.db, || {
                self.ordinary_crates()
                    .into_iter()
                    .flat_map(|krate| {
                        let mut with = krate.transitive_deps(&self.db);
                        with.push(krate);
                        with
                    })
                    .collect()
            });
        crate::process_memory::log(&format!("definition maps: crates chosen (call {call})"));
        let order: Vec<Crate> = attach_db(&self.db, || {
            Crate::all(&self.db)
                .into_iter()
                .filter(|krate| needed.contains(&krate.base()))
                .collect()
        });
        let (everything, local, library, sysroot) = attach_db(&self.db, || {
            let kinds: Vec<_> = order.iter().map(|krate| krate.origin(&self.db)).collect();
            (
                Crate::all(&self.db).len(),
                kinds.iter().filter(|origin| origin.is_local()).count(),
                kinds.iter().filter(|origin| origin.is_lib()).count(),
                kinds.iter().filter(|origin| origin.is_lang()).count(),
            )
        });
        tracing::info!(
            target: "cordial::definition_maps",
            step = %format!("chosen {call}"),
            crates = order.len(),
            everything,
            local,
            library,
            sysroot,
            "the crates whose definition maps are built, of all the crates the database holds"
        );
        let started = std::time::Instant::now();
        let status = crate::LiveStatus::global();
        let mut built = 0_u64;
        let mut released_at = crate::memory_limit::meter_kib().unwrap_or(0);
        for krate in order {
            let (queries_before, expansions_before, crate_started) = (
                status.executed(),
                status.executed_of(MACRO_EXPANSION),
                std::time::Instant::now(),
            );
            let (name, local, lang, files, source_kib) = attach_db(&self.db, || {
                let _ = ra_ap_hir_def::nameres::crate_def_map(&self.db, krate.base());
                let origin = krate.origin(&self.db);
                let (files, bytes) = source_root_size(&self.db, krate);
                (
                    krate
                        .display_name(&self.db)
                        .map_or_else(String::new, |name| name.to_string()),
                    origin.is_local(),
                    origin.is_lang(),
                    files,
                    bytes / 1024,
                )
            });
            let held = crate::memory_limit::meter_kib().unwrap_or(0);
            let memory = crate::process_memory::sample();
            let queries = status.executed().saturating_sub(queries_before);
            // A map that was already built costs nothing here and is not said again: the crates
            // reported are the ones this call built.
            if queries > 0 {
                built += 1;
            }
            if queries > 0 {
                tracing::info!(
                    target: "cordial::definition_maps",
                    krate = %format!("{name} {call}"),
                    local,
                    sysroot = lang,
                    files,
                    source_kib,
                    micros = u64::try_from(crate_started.elapsed().as_micros()).unwrap_or(u64::MAX),
                    queries,
                    expansions = status.executed_of(MACRO_EXPANSION).saturating_sub(expansions_before),
                    held_kib = held,
                    resident_kib = memory.map_or(0, |memory| memory.resident_kib()),
                    peak_kib = memory.map_or(0, |memory| memory.peak_kib()),
                    elapsed_ms = status.elapsed_ms(),
                    "a crate's definition map was built"
                );
            }
            if crate::memory_limit::scaffolding_to_go()
                && held.saturating_sub(released_at) >= crate::memory_limit::scaffolding_step_kib()
            {
                let dropped = self.jettison_scaffolding("definition maps");
                tracing::debug!(
                    target: "cordial::enumeration",
                    krate = %name,
                    released = dropped,
                    "the scaffolding built so far was let go of between definition maps"
                );
                released_at = crate::memory_limit::meter_kib().unwrap_or(0);
            }
        }
        tracing::info!(
            target: "cordial::definition_maps",
            step = %format!("built {call}"),
            crates_built = built,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            "every chosen definition map was built"
        );
        crate::process_memory::log(&format!("definition maps built (call {call})"));
        super::footprint::log(
            &mut self.db,
            &format!("definition maps built (call {call})"),
        );
    }
}

/// The word for the `index`th call (from 0). Metric names drop digits, and two calls must not
/// share one.
fn ordinal(index: u64) -> &'static str {
    const WORDS: [&str; 8] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
    ];
    usize::try_from(index)
        .ok()
        .and_then(|index| WORDS.get(index))
        .copied()
        .unwrap_or("later")
}

/// The query that parses one macro expansion: its executions count the expansions a phase made.
const MACRO_EXPANSION: &str = "MacroCallId::parse_macro_expansion_";

/// How many files the source root holding `krate`'s root file has, and their size in bytes. A
/// crate's definition map reads files of its own source root, so this says how much text a crate
/// stands on; a toolchain's libraries share one root.
#[instrument(level = "trace", skip(db))]
fn source_root_size(db: &RootDatabase, krate: Crate) -> (u64, u64) {
    use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
    let root = db.file_source_root(krate.root_file(db)).source_root_id(db);
    let (mut files, mut bytes) = (0_u64, 0_u64);
    for file in db.source_root(root).source_root(db).iter() {
        files += 1;
        bytes += db.file_text(file).text(db).len() as u64;
    }
    (files, bytes)
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
        let before = functions.len();
        functions.extend(functions_in(db, krate));
        // What the process held and what the machine saw of it once this crate's functions were
        // listed: listing them builds the crate's definition map and those of what it needs, and the
        // growth from one crate to the next says where the syntax trees and expansions come from.
        tracing::info!(
            target: "cordial::enumeration",
            krate = %krate.display_name(db).map_or_else(String::new, |name| name.to_string()),
            functions = functions.len() - before,
            held_kib = crate::memory_limit::meter_kib().unwrap_or(0),
            resident_kib = crate::memory_sample().map_or(0, |memory| memory.resident_kib()),
            "a crate's functions were listed"
        );
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
