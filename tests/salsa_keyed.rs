//! A reader of one key of a scope depends on that key and on no other, a key that is not there included.
//!
//! A tracked field is one dependency however much it holds, so a reader of one name of a module's scope runs again
//! when any other name of the scope changes. A keyed dependency is on one name: whoever makes the scope says which
//! names changed (`salsa::keyed::touch`), and a reader says which it read (`salsa::keyed::read`).

use std::cell::RefCell;
use std::collections::BTreeMap;

use salsa::plumbing::AsId;
use salsa::{Database, Durability, Storage};

/// A name, as a module's scope has them.
#[salsa::interned(revisions = usize::MAX)]
struct Name<'db> {
    text: String,
}

thread_local! {
    /// What the scope holds, outside the database: it is read without a dependency on all of it.
    static SCOPE: RefCell<BTreeMap<String, u32>> = const { RefCell::new(BTreeMap::new()) };
    /// The names a reader ran for.
    static RAN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// What the scope holds for a name: a reader of exactly that name.
#[salsa::tracked]
fn lookup<'db>(db: &'db dyn Database, name: Name<'db>) -> Option<u32> {
    salsa::keyed::read(db, name.as_id(), Durability::LOW);
    let text = name.text(db);
    RAN.with(|ran| ran.borrow_mut().push(text.clone()));
    SCOPE.with(|scope| scope.borrow().get(text.as_str()).copied())
}

impl Db {
    /// The scope changes: `then` is what it holds from the next revision on, and the names the maker says
    /// changed are touched in it.
    fn change(&mut self, then: &[(&str, u32)], touched: &[&str]) {
        SCOPE.with(|scope| {
            *scope.borrow_mut() = then.iter().map(|(name, value)| ((*name).to_owned(), *value)).collect();
        });
        self.synthetic_write(Durability::LOW);
        for text in touched {
            let name = Name::new(&*self, (*text).to_owned());
            salsa::keyed::touch(&*self, name.as_id());
        }
    }

    fn read(&self, text: &str) -> Option<u32> {
        *lookup(self, Name::new(self, text.to_owned()))
    }

    fn ran(&self) -> Vec<String> {
        RAN.with(|ran| std::mem::take(&mut *ran.borrow_mut()))
    }
}

/// What the maker says changed when `mod tests` is added to a scope of `twice` and `plain`.
enum Touches {
    /// The name that was added.
    TheName,
    /// A name nobody added, as a maker that touched the wrong thing would.
    Another,
}

fn names_run_again_after_adding_tests(touches: Touches) -> Vec<String> {
    let mut db = Db::default();
    SCOPE.with(|scope| *scope.borrow_mut() = [("twice".to_owned(), 1), ("plain".to_owned(), 2)].into());
    assert_eq!(db.read("twice"), Some(1));
    assert_eq!(db.read("plain"), Some(2));
    assert_eq!(db.read("tests"), None, "a name that is not there is read as not there");
    let _ = db.ran();

    let touched = match touches {
        Touches::TheName => "tests",
        Touches::Another => "twice",
    };
    db.change(&[("twice", 1), ("plain", 2), ("tests", 3)], &[touched]);
    let _ = (db.read("twice"), db.read("plain"), db.read("tests"));
    let mut ran = db.ran();
    ran.sort();
    ran
}

#[test]
fn adding_a_name_runs_the_reader_of_that_name_and_of_no_other() {
    assert_eq!(names_run_again_after_adding_tests(Touches::TheName), ["tests"]);
}

#[test]
fn a_reader_of_a_name_that_was_not_there_finds_it_when_it_is_added() {
    let mut db = Db::default();
    assert_eq!(db.read("tests"), None);
    db.change(&[("tests", 3)], &["tests"]);
    assert_eq!(db.read("tests"), Some(3));
}

#[test]
fn the_test_can_fail_when_the_wrong_name_is_touched() {
    // The reader of `twice` runs again and the one of `tests` does not: the keyed dependency is what decides.
    assert_eq!(names_run_again_after_adding_tests(Touches::Another), ["twice"]);
}
