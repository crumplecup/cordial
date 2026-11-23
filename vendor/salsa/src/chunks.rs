//! Saving an ingredient as the chunks the state it was restored from had, where nothing in them changed.
//!
//! A saved ingredient is a map whose entries are cut into chunks (by the tool that writes the state; Salsa
//! does not know where). The tool tells Salsa how the last state cut each ingredient, as [`PlannedChunk`]s:
//! the key of the first entry of a chunk, how many entries it has, and a digest of their keys. When the
//! memos of a run of entries that is a chunk of the plan are all as the state restored them, and the keys
//! are the ones the chunk had, the run is not flattened or encoded: the ingredient writes one entry whose
//! key says which chunk it stands for ([`REUSED_PREFIX`]), and the tool takes that chunk from the last state.

/// How the state a database was restored from cut one ingredient's entries into a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChunk {
    /// The key of the chunk's first entry.
    pub first_key: String,
    /// How many entries the chunk has.
    pub entries: u64,
    /// The digest of their keys, in order (see [`key_digest`]).
    pub keys: u64,
}

/// The start of the key of an entry that stands for a chunk of the plan: the chunk's number follows.
pub const REUSED_PREFIX: &str = "\0reused:";

/// A digest of a run of keys: the same for the same keys in the same order, on every machine.
pub fn key_digest<'a>(keys: impl IntoIterator<Item = &'a str>) -> u64 {
    keys.into_iter().fold(0xcbf2_9ce4_8422_2325_u64, |digest, key| {
        let digest = key.bytes().fold(digest, |digest, byte| {
            (digest ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        (digest ^ 0xff).wrapping_mul(0x0100_0000_01b3)
    })
}
