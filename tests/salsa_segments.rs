//! A state saved as segments, a header and one serialization per ingredient, restores to the same
//! database as one saved whole, and a state saved again from the restore is the same bytes: what a
//! save writes about an ingredient depends on the ingredient and not on the others.

use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::{Database, Setter, Storage};

#[salsa::input(persist, singleton)]
struct Source {
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

static EXECUTED: AtomicUsize = AtomicUsize::new(0);

#[salsa::tracked(returns(ref), persist)]
fn derived<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    format!("{}:{}", key.n(db), Source::get(db).value(db))
}

#[salsa::tracked(returns(ref))]
fn keys(db: &dyn Database) -> Vec<Key<'_>> {
    (0..5).map(|n| Key::new(db, n)).collect()
}

fn populate(db: &Db) {
    for key in keys(db) {
        derived(db, *key);
    }
}

/// The header and each ingredient, as the bytes a file would hold.
struct Saved {
    header: Vec<u8>,
    segments: Vec<(u32, &'static str, Vec<u8>)>,
}

fn save(db: &mut Db) -> Saved {
    <dyn Database>::verify_persisted_memos(db);
    let mut header = Vec::new();
    let mut segments = Vec::new();
    <dyn Database>::save_segments::<rmp_serde::encode::Error>(
        db,
        |part| {
            rmp_serde::encode::write(&mut header, part)?;
            Ok(())
        },
        |segment, part| {
            let mut bytes = Vec::new();
            rmp_serde::encode::write(&mut bytes, part)?;
            segments.push((segment.index, segment.name, bytes));
            Ok(())
        },
    )
    .expect("a save to memory");
    Saved { header, segments }
}

fn restore(saved: &Saved) -> Db {
    let mut db = Db::default();
    let header: salsa::SegmentHeader =
        rmp_serde::from_slice(&saved.header).expect("a header reads back");
    for (index, name, bytes) in &saved.segments {
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

#[test]
fn a_state_restored_from_segments_reuses_what_it_saved() {
    let mut db = Db::default();
    Source::new(&db, 1);
    populate(&db);
    let saved = save(&mut db);
    assert!(
        saved.segments.len() >= 3,
        "the input, the interned keys and the memos are each a segment: {:?}",
        saved.segments.iter().map(|s| s.1).collect::<Vec<_>>()
    );

    let restored = restore(&saved);
    let executed = EXECUTED.load(Ordering::SeqCst);
    populate(&restored);
    assert_eq!(
        EXECUTED.load(Ordering::SeqCst),
        executed,
        "every memo came from the segments"
    );
    assert_eq!(derived(&restored, keys(&restored)[3]).as_str(), "3:1");
}

#[test]
fn a_state_saved_again_from_a_restore_is_the_same_bytes() {
    let mut db = Db::default();
    Source::new(&db, 1);
    populate(&db);
    let first = save(&mut db);

    let mut restored = restore(&first);
    populate(&restored);
    let second = save(&mut restored);

    assert_eq!(first.header, second.header, "the runtime is the same");
    let names = |saved: &Saved| -> Vec<_> { saved.segments.iter().map(|s| (s.0, s.1)).collect() };
    assert_eq!(names(&first), names(&second), "the same segments, in order");
    for (before, after) in first.segments.iter().zip(&second.segments) {
        assert_eq!(before.2, after.2, "the segment of {} is the same", before.1);
    }
}

#[test]
fn a_change_after_a_segmented_restore_finds_what_depends_on_it_out_of_date() {
    let mut db = Db::default();
    Source::new(&db, 1);
    populate(&db);
    let saved = save(&mut db);

    let mut restored = restore(&saved);
    Source::get(&restored).set_value(&mut restored).to(2);
    let executed = EXECUTED.load(Ordering::SeqCst);
    assert_eq!(derived(&restored, keys(&restored)[3]).as_str(), "3:2");
    assert!(EXECUTED.load(Ordering::SeqCst) > executed, "it ran again");
}
