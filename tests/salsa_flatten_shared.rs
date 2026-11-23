//! A save walks what its persisted queries read once, however many ingredients read it: what a query
//! reaches does not depend on which ingredient asks, so ingredients share one flattening, and each
//! saves only the edges its own sets hold. Two persisted queries over the same non-persisted one walk
//! it once between them, where each walked it for itself.

use std::sync::{Arc, Mutex};

use salsa::{Database, Storage};
use tracing_subscriber::layer::SubscriberExt as _;

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

/// Not persisted: both persisted queries read the cell through it.
#[salsa::tracked(returns(copy))]
fn reads(db: &dyn Database, cell: Cell) -> usize {
    cell.value(db)
}

#[salsa::tracked(returns(copy), persist)]
fn first(db: &dyn Database, cell: Cell) -> usize {
    reads(db, cell) + 1
}

#[salsa::tracked(returns(copy), persist)]
fn second(db: &dyn Database, cell: Cell) -> usize {
    reads(db, cell) + 2
}

const CELLS: u64 = 50;

/// An ingredient's name, how many queries it walked, how many it found already walked, and its universe.
type Report = (String, u64, u64, u64);

#[derive(Default, Clone)]
struct Walks(Arc<Mutex<Vec<Report>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Walks {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "salsa::persistence" {
            return;
        }
        #[derive(Default)]
        struct Fields {
            ingredient: String,
            walked: u64,
            reused: u64,
            universe: u64,
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "walked" => self.walked = value,
                    "reused" => self.reused = value,
                    "universe" => self.universe = value,
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
        if let Ok(mut all) = self.0.lock() {
            all.push((
                fields.ingredient,
                fields.walked,
                fields.reused,
                fields.universe,
            ));
        }
    }
}

#[test]
fn ingredients_that_read_the_same_query_walk_it_once_between_them() {
    let walks = Walks::default();
    let subscriber = tracing_subscriber::registry().with(walks.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut db = Db::default();
    for value in 0..CELLS {
        let cell = Cell::new(&db, value as usize);
        first(&db, cell);
        second(&db, cell);
    }
    serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).expect("serializing");

    let all = walks.0.lock().expect("walks");
    let of = |name: &str| {
        all.iter()
            .find(|(ingredient, ..)| ingredient.contains(name))
            .unwrap_or_else(|| panic!("{name} reports: {all:?}"))
    };
    let (_, first_walked, _, first_universe) = of("first");
    let (_, second_walked, _, second_universe) = of("second");
    assert_eq!(
        first_walked + second_walked,
        CELLS,
        "one walk of the query for each cell, not one for each ingredient: {all:?}"
    );
    assert_eq!(*second_walked, 0, "the second takes what the first found");
    // What each saves is the edges its own sets hold, however many the save numbered.
    assert_eq!(*first_universe, CELLS);
    assert_eq!(*second_universe, CELLS);
}
