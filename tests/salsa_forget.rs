//! A memo keyed by something that no longer exists is forgotten, not verified.
//!
//! A tracked struct a query made is deleted when the query runs again without making it. A memo keyed by an
//! interned value that holds that struct is not: it stays, and a save that verifies every persisted memo runs it
//! again against a struct that is gone. Rust-analyzer's items are such values (an item's id holds its module), and
//! a cfg that is flipped and flipped back leaves the items of the modules it added in that state. Forgetting the
//! memos keyed by those items makes what read them find "no memo, so changed" without running them.

use salsa::plumbing::{AsId, FromId};
use salsa::{Database, Setter, Storage};

#[salsa::input(persist, singleton)]
struct Switch {
    #[returns(copy)]
    on: bool,
}

#[salsa::tracked(persist)]
struct Module<'db> {
    #[returns(clone)]
    name: String,
}

/// An item of a module, as an item id holds its module.
#[salsa::interned(persist, revisions = usize::MAX)]
struct Item<'db> {
    module: Module<'db>,
    n: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::tracked(returns(ref))]
fn modules<'db>(db: &'db dyn Database, switch: Switch) -> Vec<Module<'db>> {
    let mut found = vec![Module::new(db, "a".to_owned())];
    if switch.on(db) {
        found.push(Module::new(db, "b".to_owned()));
    }
    found
}

/// What is known of an item: it reads the switch, so it is stale when the switch is written, and the name of
/// the module its id holds.
#[salsa::tracked(persist)]
fn describe<'db>(db: &'db dyn Database, item: Item<'db>) -> String {
    let _ = Switch::get(db).on(db);
    format!("{}:{}", item.module(db).name(db), item.n(db))
}

#[test]
fn a_memo_of_an_item_of_a_module_that_is_gone_is_forgotten_and_not_run_again() {
    let mut db = Db::default();
    let switch = Switch::new(&db, true);
    // The ids of the items, which outlive a borrow of the database.
    let (kept, gone) = {
        let all = modules(&db, switch);
        let (kept, gone) = (Item::new(&db, all[0], 1), Item::new(&db, all[1], 2));
        assert_eq!(describe(&db, kept), "a:1");
        assert_eq!(describe(&db, gone), "b:2");
        (kept.as_id(), gone.as_id())
    };

    // The switch goes off and the module `b` is not made again: it is deleted.
    switch.set_on(&mut db).to(false);
    assert_eq!(modules(&db, switch).len(), 1);

    // What was computed for the item of the module that is gone is forgotten, and nothing else.
    let forgotten = <dyn Database>::forget_memos_for_inputs(&mut db, &[gone]);
    assert_eq!(forgotten, 1);

    // Verifying every persisted memo, as a save does, does not run the memo of the item that is gone.
    <dyn Database>::verify_persisted_memos(&db);
    assert_eq!(describe(&db, Item::from_id(kept)), "a:1");
}

#[test]
fn a_tracked_struct_says_whether_it_exists_deleted_or_replaced_without_being_read() {
    use salsa::Existence;
    let mut db = Db::default();
    let switch = Switch::new(&db, true);
    let b = {
        let all = modules(&db, switch);
        assert_eq!(<dyn Database>::existence(&db, all[0].as_id()), Existence::Exists);
        assert_eq!(<dyn Database>::existence(&db, all[1].as_id()), Existence::Exists);
        all[1].as_id()
    };

    // `b` is not made again: it is deleted, and asking says so; its fields are not read.
    switch.set_on(&mut db).to(false);
    assert_eq!(modules(&db, switch).len(), 1);
    assert_eq!(<dyn Database>::existence(&db, b), Existence::Deleted);

    // Made again, it takes the slot back with the next generation: the old id is not the new struct.
    switch.set_on(&mut db).to(true);
    let again = modules(&db, switch)[1].as_id();
    assert_ne!(again, b);
    assert_eq!(<dyn Database>::existence(&db, again), Existence::Exists);
    assert_eq!(<dyn Database>::existence(&db, b), Existence::Replaced);
}
