//! Native reverse links follow exact memo publications and semantic argument owners.

use salsa::plumbing::{AsId, FromId};
use salsa::{Database, Id, QueryOwnership, Setter, Storage};

#[salsa::input(persist)]
struct Source {
    owners: Vec<Id>,
    value: u32,
}

#[salsa::interned(persist)]
struct Arguments<'db> {
    source: Source,
    offset: u32,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::tracked(returns(ref))]
fn first(db: &dyn Database, arguments: Arguments<'_>) -> Vec<u32> {
    vec![arguments.source(db).value(db) + arguments.offset(db); 16]
}

#[salsa::tracked(returns(ref))]
fn second(db: &dyn Database, arguments: Arguments<'_>) -> Vec<u32> {
    vec![*arguments.source(db).value(db); 8]
}

fn owners(db: &dyn Database, id: Id) -> QueryOwnership {
    let arguments = Arguments::from_id(id);
    QueryOwnership::owned(arguments.source(db).owners(db).iter().copied())
}

fn configured() -> Db {
    let mut db = Db::default();
    salsa::register_query_ownership::<first, dyn Database>(&mut db, owners);
    salsa::register_query_ownership::<second, dyn Database>(&mut db, owners);
    db
}

#[test]
fn exact_functions_and_combinations_have_separate_memberships() {
    let mut db = configured();
    let root = Source::new(&db, vec![], 0);
    salsa::activate_owner_roots(&mut db, &[root.as_id(), root.as_id()]);
    let source = Source::new(&db, vec![root.as_id(), root.as_id()], 7);
    let a = Arguments::new(&db, source, 1);
    let b = Arguments::new(&db, source, 2);
    assert_eq!(first(&db, a), &[8; 16]);
    first(&db, b);
    second(&db, a);
    let a = a.as_id();
    let stats = salsa::owner_index_stats(&mut db);
    assert_eq!(
        (
            stats.roots,
            stats.live_roots,
            stats.buckets,
            stats.memberships
        ),
        (1, 1, 1, 3)
    );
    first(&db, Arguments::from_id(a));
    assert_eq!(
        salsa::owner_index_stats(&mut db),
        stats,
        "cache hits reuse links"
    );
}

#[test]
fn replacement_detaches_the_previous_owner_set() {
    let mut db = configured();
    let left = Source::new(&db, vec![], 0);
    let right = Source::new(&db, vec![], 0);
    salsa::activate_owner_roots(&mut db, &[left.as_id(), right.as_id()]);
    let source = Source::new(&db, vec![left.as_id(), right.as_id()], 7);
    first(&db, Arguments::new(&db, source, 1));
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 2);
    // Only the resolver reads this field: its reads must invalidate the memo normally.
    source.set_owners(&mut db).to(vec![right.as_id()]);
    first(&db, Arguments::new(&db, source, 1));
    let stats = salsa::owner_index_stats(&mut db);
    assert_eq!((stats.buckets, stats.memberships), (1, 1));
    source.set_owners(&mut db).to(vec![]);
    first(&db, Arguments::new(&db, source, 1));
    let stats = salsa::owner_index_stats(&mut db);
    assert_eq!(
        (stats.buckets, stats.memberships, stats.capacity),
        (0, 0, 0)
    );
}

#[test]
fn shared_and_unconfigured_queries_do_not_allocate_owner_buckets() {
    let mut db = configured();
    let source = Source::new(&db, vec![], 1);
    first(&db, Arguments::new(&db, source, 0));
    assert_eq!(salsa::owner_index_stats(&mut db).shared, 1);
    let mut plain = Db::default();
    let source = Source::new(&plain, vec![source.as_id()], 2);
    first(&plain, Arguments::new(&plain, source, 0));
    assert_eq!(salsa::owner_index_stats(&mut plain), Default::default());
}

#[test]
fn all_owners_must_finish_and_reentry_reattaches_without_an_edit() {
    let mut db = configured();
    let left = Source::new(&db, vec![], 0);
    let right = Source::new(&db, vec![], 0);
    let roots = [left.as_id(), right.as_id()];
    salsa::activate_owner_roots(&mut db, &roots);
    let source = Source::new(&db, roots.to_vec(), 7);
    let key = Arguments::new(&db, source, 1).as_id();
    first(&db, Arguments::from_id(key));
    let report = salsa::retire_owned_values(&mut db, &roots[..1]);
    assert_eq!(
        (
            report.visited,
            report.selected,
            report.protected,
            report.released
        ),
        (1, 1, 1, 0)
    );
    let report = salsa::retire_owned_values(&mut db, &roots[1..]);
    assert_eq!(
        (report.visited, report.selected, report.released),
        (1, 1, 1)
    );
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
    assert_eq!(salsa::retire_owned_values(&mut db, &roots).selected, 0);
    salsa::activate_owner_roots(&mut db, &roots);
    assert_eq!(first(&db, Arguments::from_id(key)), &[8; 16]);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 2);
    assert_eq!(salsa::retire_owned_values(&mut db, &roots).released, 1);
}

#[test]
fn unknown_owners_fail_closed_and_are_reported() {
    let mut db = configured();
    let root = Source::new(&db, vec![], 0);
    let source = Source::new(&db, vec![root.as_id()], 2);
    first(&db, Arguments::new(&db, source, 0));
    let stats = salsa::owner_index_stats(&mut db);
    assert_eq!(
        (stats.roots, stats.memberships, stats.unclassified),
        (0, 0, 1)
    );
    assert_eq!(salsa::retire_owned_values(&mut db, &[]).unclassified, 1);
}

#[test]
fn retirement_work_is_independent_of_shared_and_completed_history() {
    for contexts in [1, 4, 16] {
        for shared_entries in [1, 64] {
            for historical_contexts in [0, 32] {
                let mut db = configured();
                let shared = Source::new(&db, vec![], 17);
                let shared_keys: Vec<_> = (0..shared_entries)
                    .map(|offset| Arguments::new(&db, shared, offset).as_id())
                    .collect();
                for &key in &shared_keys {
                    first(&db, Arguments::from_id(key));
                }
                for _ in 0..historical_contexts {
                    let root = Source::new(&db, vec![], 0).as_id();
                    salsa::activate_owner_roots(&mut db, &[root]);
                    let source = Source::new(&db, vec![root], 3);
                    first(&db, Arguments::new(&db, source, 0));
                    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, 1);
                }
                let roots: Vec<_> = (0..contexts)
                    .map(|_| Source::new(&db, vec![], 0).as_id())
                    .collect();
                salsa::activate_owner_roots(&mut db, &roots);
                let expected: Vec<_> = roots
                    .iter()
                    .enumerate()
                    .map(|(index, &root)| {
                        let source = Source::new(&db, vec![root], index as u32);
                        let key = Arguments::new(&db, source, 2);
                        first(&db, key);
                        second(&db, key);
                        (key.as_id(), index as u32)
                    })
                    .collect();
                assert_eq!(salsa::owner_index_stats(&mut db).memberships, contexts * 2);
                for (retired, &root) in roots.iter().enumerate() {
                    let report = salsa::retire_owned_values(&mut db, &[root]);
                    assert_eq!(
                        (
                            report.visited,
                            report.selected,
                            report.owner_checks,
                            report.released
                        ),
                        (2, 2, 2, 2)
                    );
                    let stats = salsa::owner_index_stats(&mut db);
                    assert_eq!(
                        (stats.memberships, stats.buckets, stats.shared),
                        (
                            (contexts - retired - 1) * 2,
                            contexts - retired - 1,
                            shared_entries as usize
                        )
                    );
                    // Read the independent expected unfinished/shared populations. Hits must
                    // neither reclassify ownership nor grow the reverse index.
                    for &(key, value) in &expected[retired + 1..] {
                        assert_eq!(first(&db, Arguments::from_id(key)), &[value + 2; 16]);
                        assert_eq!(second(&db, Arguments::from_id(key)), &[value; 8]);
                    }
                    for &key in &shared_keys {
                        first(&db, Arguments::from_id(key));
                    }
                    assert_eq!(salsa::owner_index_stats(&mut db), stats);
                }
                let stats = salsa::owner_index_stats(&mut db);
                assert_eq!((stats.memberships, stats.capacity), (0, 0));
                assert_eq!(stats.roots, contexts + historical_contexts);
                let (key, value) = expected[0];
                salsa::activate_owner_roots(&mut db, &roots[..1]);
                assert_eq!(first(&db, Arguments::from_id(key)), &[value + 2; 16]);
                assert_eq!(salsa::retire_owned_values(&mut db, &roots[..1]).released, 1);
            }
        }
    }
}
