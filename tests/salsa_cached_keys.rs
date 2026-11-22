//! Select cached exact-key values without running queries or decoding saved payloads.

use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::plumbing::AsId;
use salsa::{Database, Setter, Storage};

static EXECUTIONS: AtomicUsize = AtomicUsize::new(0);

#[salsa::input(persist)]
struct Source {
    #[returns(copy)]
    value: u32,
}

#[salsa::interned(persist)]
struct Unused<'db> {
    value: u32,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::tracked(persist, returns(ref))]
fn saved(db: &dyn Database, source: Source) -> Vec<u32> {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
    vec![source.value(db); 16]
}

#[salsa::tracked(returns(copy))]
fn transient(db: &dyn Database, source: Source) -> u32 {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
    source.value(db)
}

#[salsa::tracked(returns(copy))]
fn composite(db: &dyn Database, source: Source, offset: u32) -> u32 {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
    source.value(db) + offset
}

#[test]
fn cached_keys_skip_missing_dropped_stale_and_composite_memos() {
    let mut db = Db::default();
    let selected = Source::new(&db, 1);
    let other = Source::new(&db, 2);
    let missing = Source::new(&db, 3);
    let unused = Unused::new(&db, 4).as_id();
    let inputs = [
        selected.as_id(),
        other.as_id(),
        selected.as_id(),
        missing.as_id(),
        unused,
    ];
    saved(&db, selected);
    transient(&db, other);
    composite(&db, missing, 5);
    let before = EXECUTIONS.load(Ordering::SeqCst);
    assert_eq!(
        db.cached_query_keys(&inputs, &["saved"]),
        [selected.as_id()]
    );
    assert_eq!(
        db.cached_query_keys(&inputs, &["transient"]),
        [other.as_id()]
    );
    assert_eq!(
        db.cached_query_keys(&inputs, &[]),
        [selected.as_id(), other.as_id()]
    );
    assert!(db.cached_query_keys(&[], &[]).is_empty());
    assert!(db.cached_query_keys(&inputs, &["absent"]).is_empty());
    assert_eq!(EXECUTIONS.load(Ordering::SeqCst), before);

    db.drop_regenerable_values();
    assert_eq!(db.cached_query_keys(&inputs, &[]), [selected.as_id()]);
    selected.set_value(&mut db).to(8);
    assert!(
        db.cached_query_keys(&inputs, &[]).is_empty(),
        "stale values are not validated"
    );
    assert_eq!(EXECUTIONS.load(Ordering::SeqCst), before);
    assert_eq!(saved(&db, selected), &[8; 16]);
    assert_eq!(db.cached_query_keys(&inputs, &[]), [selected.as_id()]);
    transient(&db, other);
    assert_eq!(
        db.cached_query_keys(&inputs, &[]),
        [selected.as_id(), other.as_id()]
    );
}

#[test]
fn cached_keys_include_encoded_values_without_decoding_them() {
    let mut db = Db::default();
    let source = Source::new(&db, 7);
    saved(&db, source);
    transient(&db, source);
    let encoded = rmp_serde::to_vec(&<dyn Database>::as_serialize(&mut db)).unwrap();
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut rmp_serde::Deserializer::new(encoded.as_slice()),
    )
    .unwrap();
    let before = EXECUTIONS.load(Ordering::SeqCst);
    assert_eq!(
        restored.cached_query_keys(&[source.as_id()], &["saved"]),
        [source.as_id()]
    );
    assert!(
        restored
            .cached_query_keys(&[source.as_id()], &["transient"])
            .is_empty()
    );
    let mut held = 0;
    restored.values_in_memory(false, true, &["saved"], &mut |_, count| held += count);
    assert_eq!(
        held, 0,
        "metadata inspection leaves the saved value encoded"
    );
    assert_eq!(EXECUTIONS.load(Ordering::SeqCst), before);
    assert_eq!(saved(&restored, source), &[7; 16]);
    assert_eq!(EXECUTIONS.load(Ordering::SeqCst), before);
}

#[test]
fn repeated_storage_inspection_preserves_current_cached_values() {
    let mut db = Db::default();
    let source = Source::new(&db, 7);
    saved(&db, source);
    transient(&db, source);
    let inputs = [source.as_id()];
    let revision = salsa::plumbing::current_revision(&db);
    let executions = EXECUTIONS.load(Ordering::SeqCst);

    // More inspections than the cancellation counter can hold must not age
    // untouched memos out of current-value selection.
    for _ in 0..512 {
        assert_eq!(db.tracked_output_keys(&inputs), inputs);
        assert_eq!(
            db.cached_query_keys(&inputs, &["saved", "transient"]),
            inputs
        );
        let mut all = 0;
        db.values_in_memory(false, true, &["saved", "transient"], &mut |_, count| {
            all += count;
        });
        let mut selected = 0;
        db.values_in_memory_for_inputs(
            &inputs,
            false,
            true,
            &["saved", "transient"],
            &mut |_, count| selected += count,
        );
        assert_eq!(all, 2);
        assert_eq!(selected, 2);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    assert_eq!(EXECUTIONS.load(Ordering::SeqCst), executions);
    source.set_value(&mut db).to(9);
    assert!(db.cached_query_keys(&inputs, &[]).is_empty());
    assert_eq!(saved(&db, source), &[9; 16]);
    assert_eq!(transient(&db, source), 9);
}

#[test]
fn storage_inspection_cancels_another_handle_before_reading_memos() {
    let (cancelled, wait_for_cancellation) = std::sync::mpsc::channel();
    let mut db = Db {
        storage: Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, salsa::EventKind::DidSetCancellationFlag) {
                cancelled.send(()).unwrap();
            }
        }))),
    };
    let source = Source::new(&db, 7);
    saved(&db, source);
    let other = db.clone();
    let worker = std::thread::spawn(move || {
        wait_for_cancellation
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        drop(other);
    });
    assert_eq!(
        db.cached_query_keys(&[source.as_id()], &["saved"]),
        [source.as_id()]
    );
    worker.join().unwrap();
    assert_eq!(saved(&db, source), &[7; 16]);
}
