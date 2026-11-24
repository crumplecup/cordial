//! State that belongs to one database and is not a query's result: made when it is first asked for, shared by the
//! handles of the database, and gone with it. A performance hint a query keeps for its next run (the map it made
//! before) lives here, so that two databases in one process never see each other's.

use salsa::{Database, Storage};
use std::sync::Mutex;

#[derive(Default)]
struct Notes(Mutex<Vec<u32>>);

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[test]
fn an_extension_belongs_to_a_database_and_is_shared_by_its_handles() {
    let first = Db::default();
    let second = Db::default();
    salsa::plumbing::extension::<Notes, _>(&first).0.lock().unwrap().push(1);

    let handle = first.clone();
    assert_eq!(
        *salsa::plumbing::extension::<Notes, _>(&handle).0.lock().unwrap(),
        vec![1],
        "a handle of the database sees what the database was given"
    );
    assert!(
        salsa::plumbing::extension::<Notes, _>(&second).0.lock().unwrap().is_empty(),
        "another database has its own"
    );
}
