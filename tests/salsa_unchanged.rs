//! A save says which ingredients hold what the state the database was restored from holds, so that a
//! caller can keep that state's encoding of them and not encode them again. An ingredient is unchanged
//! until something is inserted, replaced, dropped, made, written or freed in it. The claim is checked
//! the plain way: every ingredient that says it is unchanged encodes to the bytes the state had.

use salsa::{Database, Setter, Storage};

#[salsa::input(persist, singleton)]
struct Source {
    #[returns(copy)]
    value: usize,
}

#[salsa::input(persist, singleton)]
struct Other {
    #[returns(copy)]
    value: usize,
}

#[salsa::interned(persist)]
struct Key<'db> {
    n: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::tracked(returns(ref), persist)]
fn derived<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    format!("{}:{}", key.n(db), Source::get(db).value(db))
}

#[salsa::tracked(returns(ref), persist)]
fn independent<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    format!("{}:{}", key.n(db), Other::get(db).value(db))
}

#[salsa::tracked(returns(ref))]
fn keys(db: &dyn Database) -> Vec<Key<'_>> {
    (0..5).map(|n| Key::new(db, n)).collect()
}

fn populate(db: &Db) {
    for key in keys(db) {
        derived(db, *key);
        independent(db, *key);
    }
}

struct Saved {
    header: Vec<u8>,
    /// Each ingredient: its index, name, whether it said it was unchanged, and its bytes.
    segments: Vec<(u32, &'static str, bool, Vec<u8>)>,
    canonical: bool,
}

fn save(db: &mut Db) -> Saved {
    <dyn Database>::verify_persisted_memos(db);
    let mut header = Vec::new();
    let mut segments = Vec::new();
    let mut canonical = false;
    <dyn Database>::save_segments::<rmp_serde::encode::Error>(
        db,
        |part| {
            rmp_serde::encode::write(&mut header, part)?;
            Ok(())
        },
        |segment, part| {
            let mut bytes = Vec::new();
            rmp_serde::encode::write(&mut bytes, part)?;
            canonical = segment.canonical;
            segments.push((segment.index, segment.name, segment.unchanged, bytes));
            Ok(())
        },
    )
    .expect("a save to memory");
    Saved {
        header,
        segments,
        canonical,
    }
}

fn restore(saved: &Saved) -> Db {
    let mut db = Db::default();
    let header: salsa::SegmentHeader = rmp_serde::from_slice(&saved.header).expect("a header");
    for (index, name, _, bytes) in &saved.segments {
        <dyn Database>::restore_segment(
            &mut db,
            &header,
            *index,
            &mut rmp_serde::Deserializer::new(bytes.as_slice()),
        )
        .unwrap_or_else(|error| panic!("restoring {name}: {error}"));
    }
    <dyn Database>::finish_segments(&mut db, header);
    db
}

fn unchanged(saved: &Saved, name: &str) -> bool {
    saved
        .segments
        .iter()
        .find(|(_, ingredient, ..)| ingredient.contains(name))
        .unwrap_or_else(|| {
            panic!(
                "no {name} among {:?}",
                saved.segments.iter().map(|s| s.1).collect::<Vec<_>>()
            )
        })
        .2
}

/// Every ingredient that says it is unchanged encodes to the bytes of the state it was restored from.
fn assert_unchanged_means_same_bytes(first: &Saved, second: &Saved) {
    for (index, name, said, bytes) in &second.segments {
        if !said {
            continue;
        }
        let (_, _, _, before) = first
            .segments
            .iter()
            .find(|(before, ..)| before == index)
            .unwrap_or_else(|| panic!("{name} was not in the first state"));
        assert_eq!(
            bytes, before,
            "{name} said it was unchanged and encodes differently"
        );
    }
}

#[test]
fn a_restored_database_that_changed_nothing_says_every_ingredient_is_unchanged() {
    let mut db = Db::default();
    Source::new(&db, 1);
    Other::new(&db, 1);
    populate(&db);
    let first = save(&mut db);
    assert!(first.canonical);

    let mut restored = restore(&first);
    let second = save(&mut restored);
    assert!(
        second.segments.iter().all(|(_, _, said, _)| *said),
        "nothing was done: {:?}",
        second
            .segments
            .iter()
            .map(|s| (s.1, s.2))
            .collect::<Vec<_>>()
    );
    assert_unchanged_means_same_bytes(&first, &second);
}

#[test]
fn a_write_changes_the_ingredients_it_reaches_and_no_others() {
    let mut db = Db::default();
    Source::new(&db, 1);
    Other::new(&db, 1);
    populate(&db);
    let first = save(&mut db);

    let mut restored = restore(&first);
    Source::get(&restored).set_value(&mut restored).to(2);
    // Run the queries again: the ones that read what was written run again, and the others are as they were.
    populate(&restored);
    let second = save(&mut restored);

    assert!(!unchanged(&second, "Source"), "the input was written");
    assert!(!unchanged(&second, "derived"), "its memos ran again");
    assert!(unchanged(&second, "Other"), "an input nothing wrote");
    assert!(
        unchanged(&second, "independent"),
        "memos that read only that input are as they were"
    );
    assert!(unchanged(&second, "Key"), "the keys were found, not made");
    assert_unchanged_means_same_bytes(&first, &second);
}

#[test]
fn a_new_value_in_an_interner_changes_it() {
    let mut db = Db::default();
    Source::new(&db, 1);
    Other::new(&db, 1);
    populate(&db);
    let first = save(&mut db);

    let mut restored = restore(&first);
    populate(&restored);
    Key::new(&restored, 99);
    let second = save(&mut restored);
    assert!(!unchanged(&second, "Key"));
    assert!(unchanged(&second, "Source"));
    assert_unchanged_means_same_bytes(&first, &second);
}
