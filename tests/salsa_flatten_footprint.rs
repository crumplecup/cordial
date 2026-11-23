//! What a flattening holds while it runs: a set takes the runs of the numbers it holds, and not the
//! width of the universe it is among, so many small sets over a wide universe take what they hold. (They
//! were bits of every number up to the highest in the set: these 640 single-edge sets took 3,520 words
//! of 64 bits, and `elicit_doc`'s 62,768 sets over 40,377 edges 155 MB.) The save says how much, so that
//! a change to the sets can be judged by the numbers and not by a guess.

use std::sync::{Arc, Mutex};

use salsa::{Database, Storage};
use tracing_subscriber::layer::SubscriberExt as _;

/// A persisted input of its own: one edge each.
#[salsa::input(persist)]
struct Cell {
    #[returns(copy)]
    value: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// Not persisted: what the persisted query reads goes through it.
#[salsa::tracked(returns(copy))]
fn reads(db: &dyn Database, cell: Cell) -> usize {
    cell.value(db)
}

#[salsa::tracked(returns(copy), persist)]
fn total(db: &dyn Database, cell: Cell) -> usize {
    reads(db, cell) + 1
}

/// Enough cells that the highest edge needs ten words of 64 bits.
const CELLS: u64 = 640;

#[derive(Default, Clone, Debug, PartialEq, Eq)]
struct Footprint {
    memos: u64,
    sets: u64,
    set_runs: u64,
    universe: u64,
    distinct_sets: u64,
    held_runs: u64,
    held_bytes: u64,
    widest_set_runs: u64,
    kept_results: u64,
}

#[derive(Default, Clone)]
struct Reports(Arc<Mutex<Vec<(String, Footprint)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Reports {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "salsa::persistence" {
            return;
        }
        #[derive(Default)]
        struct Fields {
            ingredient: String,
            footprint: Footprint,
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                let footprint = &mut self.footprint;
                match field.name() {
                    "memos" => footprint.memos = value,
                    "sets" => footprint.sets = value,
                    "set_runs" => footprint.set_runs = value,
                    "universe" => footprint.universe = value,
                    "distinct_sets" => footprint.distinct_sets = value,
                    "held_runs" => footprint.held_runs = value,
                    "held_bytes" => footprint.held_bytes = value,
                    "widest_set_runs" => footprint.widest_set_runs = value,
                    "kept_results" => footprint.kept_results = value,
                    _ => {}
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "ingredient" {
                    self.ingredient = value.to_owned();
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Ok(mut reports) = self.0.lock() {
            reports.push((fields.ingredient, fields.footprint));
        }
    }
}

#[test]
fn a_save_says_what_its_flattening_holds() {
    let reports = Reports::default();
    let subscriber = tracing_subscriber::registry().with(reports.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut db = Db::default();
    for value in 0..CELLS {
        let cell = Cell::new(&db, value as usize);
        total(&db, cell);
    }
    serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).expect("serializing");

    let reports = reports.0.lock().expect("reports");
    let (_, held) = reports
        .iter()
        .find(|(name, _)| name.contains("total"))
        .expect("the persisted query reports");

    assert_eq!(held.memos, CELLS);
    // Each memo reads one cell through a query that is not persisted: one edge, one set of its own.
    assert_eq!(held.universe, CELLS, "an edge for every cell");
    assert_eq!(held.distinct_sets, CELLS);
    assert_eq!(
        held.sets, CELLS,
        "and the table the memos refer to holds them all"
    );
    // The cells are numbered in the order they are reached, so the set of the k-th is the number k:
    // one run of one, wherever k is among the 640.
    assert_eq!(
        held.held_runs, CELLS,
        "a run for every set, and nothing for the width of the universe"
    );
    assert_eq!(held.held_bytes, CELLS * 8);
    assert_eq!(held.widest_set_runs, 1);
    // Saved as runs, each set is one run: the cost is the edges, not the universe they are among.
    assert_eq!(held.set_runs, CELLS, "a run for every set");
    assert!(
        held.kept_results >= CELLS,
        "a result is kept for each query the walk passed through: {}",
        held.kept_results
    );
}
