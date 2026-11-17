//! A tracked struct made by a query that was not saved has the id it had in the process that saved
//! the database, when that query runs again in a process that restored it.
//!
//! Otherwise an id that a saved memo holds (rust-analyzer's module ids are tracked structs, and
//! every impl's id holds one) would name something else once the query that makes the structs was
//! run again, which it must be whenever its memo is not saved.

use salsa::{Database, Storage};

#[salsa::input(persist, singleton)]
struct Source {
    #[returns(copy)]
    value: usize,
}

/// What the creating query makes: identified by its name, the way a module is.
#[salsa::tracked(persist)]
struct Module<'db> {
    #[returns(copy)]
    name: usize,
    #[tracked]
    #[returns(copy)]
    size: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// How the creating query names what it makes.
#[derive(Clone, Copy, Debug)]
enum Naming {
    /// As it did before the save: the same identities.
    AsBefore,
    /// Differently: not the identities that were saved.
    Renamed,
    /// Another query makes them first, under the same identities, and takes what was saved.
    TakenByAnother,
}

const MODULES: usize = 8;

/// Not saved. Makes the modules, in a query of its own, as a definition map makes its modules.
#[salsa::tracked(returns(ref))]
fn modules(db: &dyn Database) -> Vec<Module<'_>> {
    let source = Source::get(db).value(db);
    (0..MODULES)
        .map(|n| Module::new(db, n, source + n))
        .collect()
}

/// Not saved, and makes the modules the same query makes, under the same identities.
#[salsa::tracked(returns(ref))]
fn the_same_modules(db: &dyn Database) -> Vec<Module<'_>> {
    let source = Source::get(db).value(db);
    (0..MODULES)
        .map(|n| Module::new(db, n, source + n))
        .collect()
}

/// Not saved either, and makes modules under other names: other identities.
#[salsa::tracked(returns(ref))]
fn renamed_modules(db: &dyn Database) -> Vec<Module<'_>> {
    let source = Source::get(db).value(db);
    (0..MODULES)
        .map(|n| Module::new(db, n + 1000, source + n))
        .collect()
}

/// Saved, and holds the id of one of the modules, as an index of impls holds a module's.
#[salsa::tracked(persist, returns(copy))]
fn mentions(db: &dyn Database) -> Module<'_> {
    modules(db)[3]
}

/// What a query that was not saved makes when it runs again in the process that restored the
/// database, against what the saved memo holds.
fn after_a_restore(naming: Naming) -> (bool, bool) {
    let mut db = Db::default();
    Source::new(&db, 5);
    mentions(&db);
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));

    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    // The saved memo, as it was.
    let kept = mentions(&restored);
    if matches!(naming, Naming::TakenByAnother) {
        the_same_modules(&restored);
    }
    // A query that was not saved, run with no earlier memo.
    let again = match naming {
        Naming::AsBefore | Naming::TakenByAnother => modules(&restored)[3],
        Naming::Renamed => renamed_modules(&restored)[3],
    };
    (kept == again, kept.size(&restored) == again.size(&restored))
}

#[test]
fn a_struct_a_query_makes_again_is_the_one_the_saved_memo_holds() {
    let (same_id, same_fields) = after_a_restore(Naming::AsBefore);
    assert!(
        same_id,
        "the module the restored memo holds is the module the query makes again"
    );
    assert!(same_fields);
}

#[test]
fn it_is_the_identity_that_finds_it_and_not_the_order_things_were_made_in() {
    // Made under other identities, nothing saved is taken: a new struct, with a new id.
    let (same_id, _) = after_a_restore(Naming::Renamed);
    assert!(
        !same_id,
        "a module with another name is not the saved module"
    );
}

#[test]
fn what_was_saved_is_taken_once() {
    // The first query to make a struct under a saved identity has the saved id; one that makes it
    // after has a new one, which is what a query with an earlier memo of its own would have found
    // had it made them first.
    let (same_id, _) = after_a_restore(Naming::TakenByAnother);
    assert!(
        !same_id,
        "the saved module was taken by the query that made it first"
    );
}

/// Where the size of a saved database is, by ingredient: what is saved is measured, and what a
/// larger ingredient holds shows as a larger share.
#[test]
fn the_size_of_a_saved_database_is_told_by_ingredient() {
    let mut db = Db::default();
    Source::new(&db, 5);
    mentions(&db);
    let sizes = <dyn Database>::ingredient_sizes(&mut db, &|part| {
        serde_json::to_vec(part).map_or(0, |encoded| encoded.len())
    });
    let size_of = |name: &str| {
        sizes
            .iter()
            .find(|(ingredient, _)| ingredient.contains(name))
            .map(|(_, bytes)| *bytes)
    };
    let modules = size_of("Module").expect("the modules are saved");
    let query = size_of("mentions").expect("the persisted query is saved");
    assert!(
        modules > query,
        "eight modules are more than the one id a query holds"
    );
    assert!(
        sizes.iter().all(|(_, bytes)| *bytes > 0),
        "everything saved has a size"
    );
}
