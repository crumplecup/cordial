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

#[test]
fn a_query_says_which_keys_it_holds_a_memo_for() {
    let mut db = Db::default();
    let switch = Switch::new(&db, true);
    let (asked, never) = {
        let all = modules(&db, switch);
        let items: Vec<_> = (0..3).map(|n| Item::new(&db, all[0], n)).collect();
        describe(&db, items[0]);
        describe(&db, items[2]);
        (vec![items[0].as_id(), items[2].as_id()], items[1].as_id())
    };
    let mut held = <dyn Database>::memo_keys(&mut db, &["describe"]);
    held.sort_by_key(|id| id.as_bits());
    let mut asked = asked;
    asked.sort_by_key(|id| id.as_bits());
    assert_eq!(held, asked, "the two that were asked, and not the one that was not");
    assert!(!held.contains(&never));
}

/// What does not read the switch.
#[salsa::tracked(persist)]
fn stable<'db>(db: &'db dyn Database, item: Item<'db>) -> usize {
    item.n(db) * 10
}

#[test]
fn what_changed_while_a_switch_was_on_is_forgotten_and_what_did_not_is_kept() {
    let mut db = Db::default();
    let switch = Switch::new(&db, false);
    let kept = {
        let all = modules(&db, switch);
        let kept = Item::new(&db, all[0], 1);
        assert_eq!(describe(&db, kept), "a:1");
        assert_eq!(*stable(&db, kept), 10);
        kept.as_id()
    };
    let before = salsa::plumbing::current_revision(&db);

    // The switch is on for a while: `b` exists, and what reads the switch is computed for it.
    switch.set_on(&mut db).to(true);
    let gone = {
        let all = modules(&db, switch);
        let (kept, gone) = (Item::from_id(kept), Item::new(&db, all[1], 2));
        assert_eq!(describe(&db, gone), "b:2");
        // Verified again with the switch on, and the same: it did not change.
        assert_eq!(describe(&db, kept), "a:1");
        gone.as_id()
    };
    switch.set_on(&mut db).to(false);
    let _ = modules(&db, switch);

    let held = |db: &mut Db, name: &str| <dyn Database>::memo_keys(db, &[name]);
    assert!(held(&mut db, "describe").contains(&gone));
    let forgotten = <dyn Database>::forget_memos_changed_after(&mut db, before);
    assert!(forgotten >= 1, "what was made while the switch was on is forgotten");
    assert!(!held(&mut db, "describe").contains(&gone));
    assert!(held(&mut db, "describe").contains(&kept), "a value that did not change is kept");
    assert!(held(&mut db, "stable").contains(&kept));
    // What made tracked structs is not forgotten: the structs are what other things hold ids of, and are the
    // same ones when the query runs again, which they would not be without its memo.
    assert!(
        !held(&mut db, "modules").is_empty(),
        "a query that makes tracked structs keeps its memo"
    );
}

/// Records the events about reads of tracked structs that are gone.
#[derive(Clone, Default)]
struct DeadReads(std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for DeadReads {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Fields(String, String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "struct_kind" => self.0 = format!("{value:?}"),
                    "state" => self.1 = format!("{value:?}"),
                    // The queries that were running, which says who read it.
                    "queries" => self.1.push_str(&format!(" in {value:?}")),
                    _ => {}
                }
            }
        }
        if event.metadata().target() != "salsa::dead_read" {
            return;
        }
        let mut fields = Fields(String::new(), String::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push((fields.0, fields.1));
    }
}

#[test]
fn a_read_of_a_tracked_struct_that_is_gone_says_which_before_it_fails() {
    use tracing_subscriber::layer::SubscriberExt;
    let mut db = Db::default();
    let switch = Switch::new(&db, true);
    // An item of the module that will be gone, which nothing has asked about yet.
    let (b, item) = {
        let all = modules(&db, switch);
        (all[1].as_id(), Item::new(&db, all[1], 9).as_id())
    };
    switch.set_on(&mut db).to(false);
    assert_eq!(modules(&db, switch).len(), 1);
    assert_eq!(<dyn Database>::existence(&db, b), salsa::Existence::Deleted);

    let reads = DeadReads::default();
    let subscriber = tracing_subscriber::registry().with(reads.clone());
    // The read of what is gone is a panic, as it always was; what is new is that the log says what it was, and
    // which query was reading it.
    let failed = tracing::subscriber::with_default(subscriber, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            describe(&db, Item::from_id(item)).clone()
        }))
        .is_err()
    });
    assert!(failed);
    let said = reads.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, "\"Module\"");
    assert!(said[0].1.starts_with("Deleted"), "{said:?}");
    assert!(said[0].1.contains("describe"), "the query that read it is named: {said:?}");
}

/// Records the `cordial::panic` events: the queries that were running, and the message.
#[derive(Clone, Default)]
struct Panics(std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Panics {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Fields(String, String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "queries" => self.0 = format!("{value:?}"),
                    "message" => self.1 = format!("{value:?}"),
                    _ => {}
                }
            }
        }
        if event.metadata().target() == "cordial::panic" {
            let mut fields = Fields(String::new(), String::new());
            event.record(&mut fields);
            self.0.lock().unwrap().push((fields.0, fields.1));
        }
    }
}

#[test]
fn a_panic_in_a_query_says_which_queries_were_running() {
    use tracing_subscriber::layer::SubscriberExt;
    cordial::install_panic_event();
    let mut db = Db::default();
    let switch = Switch::new(&db, true);
    let item = {
        let all = modules(&db, switch);
        Item::new(&db, all[1], 9).as_id()
    };
    switch.set_on(&mut db).to(false);
    assert_eq!(modules(&db, switch).len(), 1);

    let panics = Panics::default();
    let subscriber = tracing_subscriber::registry().with(panics.clone());
    let failed = tracing::subscriber::with_default(subscriber, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| describe(&db, Item::from_id(item)).clone()))
            .is_err()
    });
    assert!(failed);
    let said = panics.0.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].0.contains("describe"), "the query that was running is named: {said:?}");
    assert!(!said[0].1.is_empty(), "the message is there: {said:?}");
}
