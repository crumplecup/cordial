//! An ingredient saved as content-addressed chunks: it reads back as the map it was, a chunk the last
//! state had is linked and not written again, and a change moves only the chunks it is in.

use std::collections::BTreeMap;

use cordial::{CHUNKS, ChunkDir, ChunkError, ChunkReadError, chunked_reader, write_chunked};
use serde::Deserialize;

type Table = BTreeMap<u64, String>;

fn table(entries: u64) -> Table {
    (0..entries)
        .map(|key| (key, format!("the value of {key}")))
        .collect()
}

/// A state directory holding `table`, saved over `previous` (which may not exist).
fn save(
    table: &Table,
    previous: &std::path::Path,
    state: &std::path::Path,
) -> (cordial::Chunked, ChunkDir) {
    let mut chunks = ChunkDir::new(previous.join(CHUNKS), state.join(CHUNKS)).expect("a directory");
    let written = write_chunked(table, &mut chunks, &[]).expect("a map is chunked");
    (written, chunks)
}

fn read(state: &std::path::Path, saved: &cordial::Chunked) -> Table {
    let reader = chunked_reader(&state.join(CHUNKS), saved.entries, &saved.chunks).expect("chunks");
    Table::deserialize(&mut rmp_serde::Deserializer::new(reader)).expect("a map is read")
}

#[test]
fn a_map_reads_back_as_it_was_whatever_its_size() {
    // The sizes where the map header changes length: a fixed map, then 16 bits, then 32.
    for entries in [0, 1, 15, 16, 70, 65_535, 65_536, 70_000] {
        let dir = tempfile::tempdir().expect("a directory");
        let saved_table = table(entries);
        let (saved, _) = save(&saved_table, &dir.path().join("none"), dir.path());
        assert_eq!(saved.entries, entries);
        assert_eq!(read(dir.path(), &saved), saved_table, "{entries} entries");
    }
}

#[test]
fn what_the_last_state_had_is_linked_and_not_written() {
    let before = tempfile::tempdir().expect("a directory");
    let after = tempfile::tempdir().expect("a directory");
    let saved_table = table(5_000);
    let (first, written) = save(&saved_table, &before.path().join("none"), before.path());
    assert!(written.written > 1, "the map is more than one chunk");
    assert_eq!(written.linked, 0);

    let (second, again) = save(&saved_table, before.path(), after.path());
    assert_eq!(second.chunks, first.chunks, "the same chunks");
    assert_eq!(again.written, 0, "nothing was written");
    assert_eq!(again.linked, written.written);
    assert_eq!(read(after.path(), &second), saved_table);
}

#[test]
fn a_change_moves_only_the_chunks_it_is_in() {
    let before = tempfile::tempdir().expect("a directory");
    let after = tempfile::tempdir().expect("a directory");
    let mut changing = table(20_000);
    let (first, written) = save(&changing, &before.path().join("none"), before.path());
    let chunks = written.written;
    assert!(chunks >= 100, "{chunks} chunks to tell apart");

    // One value grows a great deal: where the chunks after it end does not depend on its size.
    changing.insert(12_345, "x".repeat(50_000));
    let (second, again) = save(&changing, before.path(), after.path());
    let differing = second
        .chunks
        .iter()
        .filter(|chunk| !first.chunks.contains(chunk))
        .count();
    assert!(differing <= 3, "{differing} chunks of {chunks} differ");
    assert_eq!(again.written as usize, differing);
    assert_eq!(read(after.path(), &second), changing);
}

#[test]
fn large_entries_are_chunks_of_their_own() {
    let dir = tempfile::tempdir().expect("a directory");
    let big: Table = (0..4)
        .map(|key| (key, "y".repeat(2 << 20) + &key.to_string()))
        .collect();
    let (saved, _) = save(&big, &dir.path().join("none"), dir.path());
    assert_eq!(saved.chunks.len(), 4);
    assert_eq!(read(dir.path(), &saved), big);
}

#[test]
fn something_that_is_not_a_map_is_refused_and_says_what_it_was() {
    let dir = tempfile::tempdir().expect("a directory");
    let mut chunks =
        ChunkDir::new(dir.path().join("none"), dir.path().join(CHUNKS)).expect("a directory");
    let error = write_chunked(&vec![1_u8, 2, 3], &mut chunks, &[]).expect_err("a sequence");
    assert!(
        matches!(error, ChunkError::NotAMap { found: "sequence" }),
        "{error}"
    );
}

#[test]
fn a_chunk_that_was_cut_short_is_found_out_before_it_is_read() {
    let dir = tempfile::tempdir().expect("a directory");
    let (saved, _) = save(&table(500), &dir.path().join("none"), dir.path());
    let victim = &saved.chunks[0];
    let path = dir.path().join(CHUNKS).join(&victim.hash);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open")
        .set_len(victim.bytes - 1)
        .expect("truncate");
    let Err(error) = chunked_reader(&dir.path().join(CHUNKS), saved.entries, &saved.chunks) else {
        panic!("a short chunk is refused");
    };
    assert!(
        matches!(error, ChunkReadError::Length { found, expected, .. }
            if found + 1 == expected && expected == victim.bytes),
        "{error}"
    );
}
