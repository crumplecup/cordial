//! A library file's text is saved as where it is on the disk and what it was, not as its text; the
//! state that restores it leaves it on the disk until something asks, and a load that finds the file
//! as it was leaves it alone while one that finds it changed writes it.

use std::path::Path;

use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{ColdTextError, FileContents, SourceDatabase};
use ra_ap_vfs::FileId;

const MARKER: &str = "pub fn a_marker_that_only_the_text_contains() {}\n";

/// The state as segments, with each file's bytes.
struct Saved {
    header: Vec<u8>,
    segments: Vec<(u32, Vec<u8>)>,
    files: Vec<u8>,
}

fn save(db: &mut RootDatabase) -> Saved {
    let mut header = Vec::new();
    let mut segments = Vec::new();
    <dyn salsa::Database>::save_segments::<rmp_serde::encode::Error>(
        db,
        |part| rmp_serde::encode::write(&mut header, part),
        |segment, part| {
            let mut bytes = Vec::new();
            rmp_serde::encode::write(&mut bytes, part)?;
            segments.push((segment.index, bytes));
            Ok(())
        },
    )
    .expect("a save to memory");
    let files = rmp_serde::to_vec(&db.files().dump()).expect("the files");
    Saved {
        header,
        segments,
        files,
    }
}

fn restore(saved: &Saved) -> RootDatabase {
    let mut db = RootDatabase::empty();
    let header: salsa::SegmentHeader = rmp_serde::from_slice(&saved.header).expect("a header");
    for (index, bytes) in &saved.segments {
        <dyn salsa::Database>::restore_segment(
            &mut db,
            &header,
            *index,
            &mut rmp_serde::Deserializer::new(bytes.as_slice()),
        )
        .expect("a segment");
    }
    <dyn salsa::Database>::finish_segments(&mut db, header);
    db.files()
        .restore(rmp_serde::from_slice(&saved.files).expect("the files"));
    db
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn revision(db: &RootDatabase) -> salsa::Revision {
    salsa::plumbing::ZalsaDatabase::zalsa(db).current_revision()
}

fn contents(db: &RootDatabase, file: FileId) -> FileContents {
    db.file_text(file).contents(db)
}

/// A database holding `MARKER` for file 0, which is on the disk at `path` when `on_disk`.
fn database(path: &Path, on_disk: bool) -> RootDatabase {
    let mut db = RootDatabase::empty();
    if on_disk {
        std::fs::write(path, MARKER).expect("a file");
    }
    db.set_file_text(FileId::from_raw(0), MARKER);
    db
}

#[test]
fn a_text_with_a_source_is_saved_as_a_reference_and_comes_back_cold() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("lib.rs");
    let mut db = database(&path, true);
    assert!(contents(&db, FileId::from_raw(0)).attach_source(&path));
    let saved = save(&mut db);
    assert!(
        !saved
            .segments
            .iter()
            .any(|(_, bytes)| contains(bytes, MARKER)),
        "no segment holds the text"
    );

    let restored = restore(&saved);
    let contents = contents(&restored, FileId::from_raw(0));
    assert!(!contents.is_resident(), "the text stayed on the disk");
    assert_eq!(contents.len(), MARKER.len());
    assert_eq!(&*contents.get(), MARKER, "and is read when it is asked for");
}

#[test]
fn a_text_without_a_source_is_saved_with_its_text() {
    let dir = tempfile::tempdir().expect("a directory");
    let mut db = database(&dir.path().join("none.rs"), false);
    assert!(
        !contents(&db, FileId::from_raw(0)).attach_source(&dir.path().join("none.rs")),
        "a file that is not on the disk has no source"
    );
    let saved = save(&mut db);
    assert!(
        saved
            .segments
            .iter()
            .any(|(_, bytes)| contains(bytes, MARKER))
    );
    assert!(contents(&restore(&saved), FileId::from_raw(0)).is_resident());
}

#[test]
fn a_text_on_the_disk_with_another_length_has_no_source() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("lib.rs");
    let db = database(&path, true);
    std::fs::write(&path, format!("{MARKER}// longer\n")).expect("a file");
    assert!(!contents(&db, FileId::from_raw(0)).attach_source(&path));
}

#[test]
fn a_load_that_finds_the_file_as_it_was_leaves_the_restored_text_alone() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("lib.rs");
    let mut db = database(&path, true);
    contents(&db, FileId::from_raw(0)).attach_source(&path);
    let mut restored = restore(&save(&mut db));
    let before = revision(&restored);

    restored.set_file_text(FileId::from_raw(0), MARKER);
    assert_eq!(revision(&restored), before, "nothing was written");
    assert!(
        !contents(&restored, FileId::from_raw(0)).is_resident(),
        "and nothing was read into memory"
    );
}

#[test]
fn a_load_that_finds_the_file_changed_writes_it_even_at_the_same_length() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("lib.rs");
    let mut db = database(&path, true);
    contents(&db, FileId::from_raw(0)).attach_source(&path);
    let mut restored = restore(&save(&mut db));
    let before = revision(&restored);

    let changed = MARKER.replace("marker", "markes");
    assert_eq!(changed.len(), MARKER.len());
    restored.set_file_text(FileId::from_raw(0), &changed);
    assert!(revision(&restored) > before, "what read it is invalidated");
    assert_eq!(&*contents(&restored, FileId::from_raw(0)).get(), changed);
}

#[test]
fn a_cold_text_says_why_it_cannot_be_given_when_the_file_changed() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("lib.rs");
    let mut db = database(&path, true);
    contents(&db, FileId::from_raw(0)).attach_source(&path);
    let restored = restore(&save(&mut db));
    let cold = contents(&restored, FileId::from_raw(0));

    std::fs::write(&path, MARKER.replace("marker", "markes")).expect("a file");
    assert!(matches!(
        cold.try_get(),
        Err(ColdTextError::ContentChanged { .. })
    ));
    std::fs::write(&path, "short").expect("a file");
    assert!(matches!(
        cold.try_get(),
        Err(ColdTextError::LengthChanged { was, is: 5, .. }) if was == MARKER.len()
    ));
    std::fs::remove_file(&path).expect("removed");
    assert!(matches!(
        cold.try_get(),
        Err(ColdTextError::Unreadable { .. })
    ));
}
