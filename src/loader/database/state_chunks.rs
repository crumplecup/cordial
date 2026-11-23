//! The Salsa state's ingredients as content-addressed chunks.
//!
//! An ingredient is saved as a map with an entry for each slot or memo. Written as a whole, a state
//! that changed one memo rewrites every byte of the ingredient. Here the entries are grouped into
//! chunks, each stored under the SHA-256 of its bytes: a chunk that the state saved before is linked
//! into the new state and not written again, so a save writes what changed and not what it passed
//! through.
//!
//! Where a chunk ends depends only on the entries in it: after an entry whose key hashes to the
//! boundary, or when the chunk is large. An entry that grows or shrinks moves the chunks after it
//! by nothing, so they are still the chunks they were. A restore reads the chunks of an ingredient in
//! order behind a map header, which is the bytes the ingredient was encoded as before there were
//! chunks.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::ser::{Impossible, SerializeMap};
use sha2::{Digest, Sha256};
use tracing::instrument;

/// One entry in this many, by its key, ends a chunk: the chunks of an ingredient are this many
/// entries on average.
const KEY_MODULUS: u64 = 64;

/// A chunk that has grown to this many bytes ends after the entry that grew it, however its keys
/// hash: a few large entries are not one chunk with the rest.
const MAX_CHUNK_BYTES: usize = 1 << 20;

/// How much of a chunk is buffered while it is read.
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// The directory of the state that holds the chunks.
pub const CHUNKS: &str = "chunks";

/// A chunk as the manifest names it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChunkEntry {
    /// The SHA-256 of the chunk's bytes, in hex: its file name.
    pub hash: String,
    /// How long the chunk is.
    pub bytes: u64,
    /// How many entries the chunk has, the key of the first and a digest of their keys: how the chunk
    /// was cut, which a save that finds the entries as they were takes the chunk by (none for a state
    /// that did not say).
    #[serde(default)]
    pub entries: u64,
    /// The key of the chunk's first entry.
    #[serde(default)]
    pub first_key: String,
    /// The digest of the keys of its entries (see [`salsa::key_digest`]).
    #[serde(default)]
    pub keys: u64,
}

/// What was written of an ingredient: how many entries its map has, and the chunks they are in.
#[derive(Debug, Default)]
pub struct Chunked {
    /// How many entries the map has.
    pub entries: u64,
    /// The chunks the entries are in, in order.
    pub chunks: Vec<ChunkEntry>,
}

/// Why an ingredient could not be saved as chunks.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum ChunkError {
    /// Something other than a map was given: every ingredient is saved as one.
    #[display("an ingredient is saved as a map, and this was a {found}")]
    NotAMap {
        /// What it was.
        found: &'static str,
    },
    /// An entry could not be encoded.
    #[display("could not encode an entry: {source}")]
    Encoding {
        /// Why.
        source: rmp_serde::encode::Error,
    },
    /// A chunk could not be put in the state.
    #[display("could not store chunk {hash}: {source}")]
    Store {
        /// The chunk.
        hash: String,
        /// Why.
        source: std::io::Error,
    },
    /// An entry stood for a chunk of the last state that it did not have.
    #[display("an entry stands for chunk {index} of the last state, which has no such chunk")]
    NoSuchChunk {
        /// Which.
        index: usize,
    },
    /// An entry stood for a chunk of the last state that is no longer there.
    #[display("chunk {hash} of the last state is gone")]
    Gone {
        /// The chunk.
        hash: String,
    },
    /// The value being saved said it could not be.
    #[display("{message}")]
    Serializing {
        /// What it said.
        message: String,
    },
}

impl serde::ser::Error for ChunkError {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::Serializing {
            message: message.to_string(),
        }
    }
}

/// Where a state's chunks go, and where the state it replaces keeps its own.
#[derive(Debug)]
pub struct ChunkDir {
    previous: PathBuf,
    /// Another state whose chunks are linked when the one before has none: a base.
    also: Option<PathBuf>,
    current: PathBuf,
    /// How many chunks were written.
    pub written: u64,
    /// How many chunks the last state had, and were linked.
    pub linked: u64,
    /// How many chunks an earlier ingredient of this state had the same bytes as.
    pub repeated: u64,
    /// How many bytes were written.
    pub bytes_written: u64,
    /// How many bytes were linked and not written.
    pub bytes_linked: u64,
    /// How many chunks an ingredient left to the last state, entry by entry, without encoding their entries.
    pub taken: u64,
}

impl ChunkDir {
    /// Chunks go to `current`, which is made; those that `previous` has are linked from it.
    #[instrument(level = "trace")]
    pub fn new(previous: PathBuf, current: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&current)?;
        Ok(Self {
            previous,
            also: None,
            current,
            written: 0,
            linked: 0,
            repeated: 0,
            bytes_written: 0,
            bytes_linked: 0,
            taken: 0,
        })
    }

    /// Link the chunks of `other` too, when the state before has not got them.
    #[must_use]
    pub fn also_linking_from(mut self, other: PathBuf) -> Self {
        self.also = Some(other);
        self
    }

    /// Whether every chunk of `chunks` is in the state before or the base: an ingredient can be left to them.
    #[must_use]
    pub fn has_all(&self, chunks: &[ChunkEntry]) -> bool {
        chunks.iter().all(|chunk| {
            [Some(&self.previous), self.also.as_ref()]
                .into_iter()
                .flatten()
                .any(|directory| directory.join(&chunk.hash).is_file())
        })
    }

    /// Put the chunks of an ingredient that the state before (or the base) has in this state without
    /// encoding it: each is linked from there. Returns whether they all were, which is not so when
    /// the state they were restored from has since been replaced by one without them.
    #[instrument(level = "trace", skip_all)]
    pub fn reuse(&mut self, chunks: &[ChunkEntry]) -> bool {
        for chunk in chunks {
            let target = self.current.join(&chunk.hash);
            if target.exists() {
                self.repeated += 1;
                continue;
            }
            let linked = [Some(&self.previous), self.also.as_ref()]
                .into_iter()
                .flatten()
                .map(|directory| directory.join(&chunk.hash))
                .any(|before| {
                    before.is_file()
                        && (std::fs::hard_link(&before, &target).is_ok() || copy(&before, &target))
                });
            if !linked {
                return false;
            }
            self.linked += 1;
            self.bytes_linked += chunk.bytes;
        }
        true
    }

    /// Put the chunk in the state: linked from the state before when that had it, written if not.
    fn store(&mut self, hash: &str, bytes: &[u8]) -> std::io::Result<()> {
        let target = self.current.join(hash);
        let length = bytes.len() as u64;
        if target.exists() {
            self.repeated += 1;
            return Ok(());
        }
        // A copy when the filesystem cannot link, which is still no encoding or hashing to do again.
        let have = [Some(&self.previous), self.also.as_ref()]
            .into_iter()
            .flatten()
            .map(|directory| directory.join(hash))
            .find(|before| {
                before.is_file()
                    && (std::fs::hard_link(before, &target).is_ok() || copy(before, &target))
            });
        if have.is_some() {
            self.linked += 1;
            self.bytes_linked += length;
            return Ok(());
        }
        std::fs::write(&target, bytes)?;
        self.written += 1;
        self.bytes_written += length;
        Ok(())
    }
}

fn copy(from: &Path, to: &Path) -> bool {
    std::fs::copy(from, to).is_ok()
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// FNV-1a: the same on every machine and run, as where a chunk ends must be.
fn key_hash(key: &[u8]) -> u64 {
    key.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Save `part`, an ingredient, as chunks in `store`.
#[instrument(level = "trace", skip_all)]
///
/// `previous` is how the last state cut the ingredient: an entry whose key says it stands for chunk `n` of
/// it (see [`salsa::REUSED_PREFIX`]) is that chunk, linked as it was and not written.
pub fn write_chunked<T: serde::Serialize + ?Sized>(
    part: &T,
    store: &mut ChunkDir,
    previous: &[ChunkEntry],
) -> Result<Chunked, ChunkError> {
    part.serialize(MapChunker { store, previous })
}

/// A serializer that takes a map and writes its entries as chunks.
struct MapChunker<'a> {
    store: &'a mut ChunkDir,
    previous: &'a [ChunkEntry],
}

macro_rules! not_a_map {
    ($($method:ident($($argument:ty)?) => $found:literal),* $(,)?) => {
        $(fn $method(self $(, _: $argument)?) -> Result<Self::Ok, Self::Error> {
            Err(ChunkError::NotAMap { found: $found })
        })*
    };
}

impl<'a> serde::Serializer for MapChunker<'a> {
    type Ok = Chunked;
    type Error = ChunkError;
    type SerializeSeq = Impossible<Chunked, ChunkError>;
    type SerializeTuple = Impossible<Chunked, ChunkError>;
    type SerializeTupleStruct = Impossible<Chunked, ChunkError>;
    type SerializeTupleVariant = Impossible<Chunked, ChunkError>;
    type SerializeMap = MapChunks<'a>;
    type SerializeStruct = Impossible<Chunked, ChunkError>;
    type SerializeStructVariant = Impossible<Chunked, ChunkError>;

    not_a_map! {
        serialize_bool(bool) => "bool",
        serialize_i8(i8) => "number",
        serialize_i16(i16) => "number",
        serialize_i32(i32) => "number",
        serialize_i64(i64) => "number",
        serialize_u8(u8) => "number",
        serialize_u16(u16) => "number",
        serialize_u32(u32) => "number",
        serialize_u64(u64) => "number",
        serialize_f32(f32) => "number",
        serialize_f64(f64) => "number",
        serialize_char(char) => "char",
        serialize_str(&str) => "string",
        serialize_bytes(&[u8]) => "byte string",
        serialize_none() => "none",
        serialize_unit() => "unit",
        serialize_unit_struct(&'static str) => "unit struct",
    }

    fn serialize_some<T: ?Sized + serde::Serialize>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(ChunkError::NotAMap {
            found: "unit variant",
        })
    }

    fn serialize_newtype_struct<T: ?Sized + serde::Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + serde::Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(ChunkError::NotAMap {
            found: "newtype variant",
        })
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(ChunkError::NotAMap { found: "sequence" })
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(ChunkError::NotAMap { found: "tuple" })
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(ChunkError::NotAMap {
            found: "tuple struct",
        })
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(ChunkError::NotAMap {
            found: "tuple variant",
        })
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapChunks {
            store: self.store,
            previous: self.previous,
            chunk: Vec::new(),
            key_hash: 0,
            keys: Vec::new(),
            reusing: None,
            done: Chunked::default(),
        })
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(ChunkError::NotAMap { found: "struct" })
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(ChunkError::NotAMap {
            found: "struct variant",
        })
    }
}

/// The entries of a map, encoded one after another into the chunk being made.
struct MapChunks<'a> {
    store: &'a mut ChunkDir,
    previous: &'a [ChunkEntry],
    chunk: Vec<u8>,
    /// The hash of the key of the entry being written.
    key_hash: u64,
    /// The keys of the entries in the chunk being made.
    keys: Vec<String>,
    /// The chunk of the last state the entry being written stands for.
    reusing: Option<usize>,
    done: Chunked,
}

/// The text of a key that was written as a MessagePack string.
fn key_text(bytes: &[u8]) -> Option<&str> {
    let (length, offset) = match *bytes.first()? {
        marker @ 0xa0..=0xbf => (usize::from(marker & 0x1f), 1),
        0xd9 => (usize::from(*bytes.get(1)?), 2),
        0xda => (
            usize::from(u16::from_be_bytes([*bytes.get(1)?, *bytes.get(2)?])),
            3,
        ),
        0xdb => (
            u32::from_be_bytes([
                *bytes.get(1)?,
                *bytes.get(2)?,
                *bytes.get(3)?,
                *bytes.get(4)?,
            ]) as usize,
            5,
        ),
        _ => return None,
    };
    std::str::from_utf8(bytes.get(offset..offset + length)?).ok()
}

impl MapChunks<'_> {
    fn flush(&mut self) -> Result<(), ChunkError> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        let hash = hex(&Sha256::digest(&self.chunk));
        self.store
            .store(&hash, &self.chunk)
            .map_err(|source| ChunkError::Store {
                hash: hash.clone(),
                source,
            })?;
        self.done.chunks.push(ChunkEntry {
            hash,
            bytes: self.chunk.len() as u64,
            entries: self.keys.len() as u64,
            first_key: self.keys.first().cloned().unwrap_or_default(),
            keys: salsa::key_digest(self.keys.iter().map(String::as_str)),
        });
        self.keys.clear();
        self.chunk.clear();
        Ok(())
    }
}

impl SerializeMap for MapChunks<'_> {
    type Ok = Chunked;
    type Error = ChunkError;

    fn serialize_key<T: ?Sized + serde::Serialize>(&mut self, key: &T) -> Result<(), ChunkError> {
        let start = self.chunk.len();
        rmp_serde::encode::write(&mut self.chunk, key)
            .map_err(|source| ChunkError::Encoding { source })?;
        self.key_hash = key_hash(&self.chunk[start..]);
        let text = key_text(&self.chunk[start..]).map(str::to_owned);
        if let Some(index) = text
            .as_deref()
            .and_then(|text| text.strip_prefix(salsa::REUSED_PREFIX))
            .and_then(|index| index.parse::<usize>().ok())
        {
            // The entry stands for a chunk of the last state: what came before it is a chunk of its own.
            self.chunk.truncate(start);
            self.flush()?;
            self.reusing = Some(index);
        } else {
            self.keys.push(text.unwrap_or_default());
        }
        Ok(())
    }

    fn serialize_value<T: ?Sized + serde::Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), ChunkError> {
        if let Some(index) = self.reusing.take() {
            let chunk = self
                .previous
                .get(index)
                .ok_or(ChunkError::NoSuchChunk { index })?
                .clone();
            if !self.store.reuse(std::slice::from_ref(&chunk)) {
                return Err(ChunkError::Gone { hash: chunk.hash });
            }
            self.store.taken += 1;
            self.done.entries += chunk.entries;
            self.done.chunks.push(chunk);
            return Ok(());
        }
        rmp_serde::encode::write(&mut self.chunk, value)
            .map_err(|source| ChunkError::Encoding { source })?;
        self.done.entries += 1;
        if self.key_hash.is_multiple_of(KEY_MODULUS) || self.chunk.len() >= MAX_CHUNK_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    fn end(mut self) -> Result<Chunked, ChunkError> {
        self.flush()?;
        Ok(self.done)
    }
}

/// The bytes of a map header for `entries` entries, as MessagePack writes it.
fn map_header(entries: u64) -> Vec<u8> {
    match u32::try_from(entries) {
        Ok(entries @ 0..=15) => vec![0x80 | entries as u8],
        Ok(entries @ 16..=0xffff) => {
            let mut header = vec![0xde];
            header.extend_from_slice(&(entries as u16).to_be_bytes());
            header
        }
        Ok(entries) => {
            let mut header = vec![0xdf];
            header.extend_from_slice(&entries.to_be_bytes());
            header
        }
        // Past what MessagePack can count: a state this large is not one.
        Err(_) => vec![0xdf, 0xff, 0xff, 0xff, 0xff],
    }
}

/// Why the chunks of an ingredient cannot be read.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum ChunkReadError {
    /// A chunk cannot be read.
    #[display("chunk {hash} is unreadable: {source}")]
    Unreadable {
        /// The chunk.
        hash: String,
        /// Why.
        source: std::io::Error,
    },
    /// A chunk is not as long as the manifest says.
    #[display("chunk {hash} is {found} bytes where the manifest says {expected}")]
    Length {
        /// The chunk.
        hash: String,
        /// What the manifest says.
        expected: u64,
        /// What the file is.
        found: u64,
    },
}

/// The bytes of an ingredient as one map: a header and then its chunks, read as they are asked for.
/// Their lengths are checked first, so a chunk cut short is found out before anything is decoded.
#[instrument(level = "trace", skip(chunks))]
pub fn chunked_reader(
    dir: &Path,
    entries: u64,
    chunks: &[ChunkEntry],
) -> Result<ChunkedReader, ChunkReadError> {
    for chunk in chunks {
        let found = std::fs::metadata(dir.join(&chunk.hash))
            .map_err(|source| ChunkReadError::Unreadable {
                hash: chunk.hash.clone(),
                source,
            })?
            .len();
        if found != chunk.bytes {
            return Err(ChunkReadError::Length {
                hash: chunk.hash.clone(),
                expected: chunk.bytes,
                found,
            });
        }
    }
    Ok(ChunkedReader {
        header: std::io::Cursor::new(map_header(entries)),
        remaining: chunks
            .iter()
            .rev()
            .map(|chunk| dir.join(&chunk.hash))
            .collect(),
        open: None,
    })
}

/// The bytes of an ingredient as one map, read from its chunks.
pub struct ChunkedReader {
    header: std::io::Cursor<Vec<u8>>,
    /// The chunks not yet opened, the next last.
    remaining: Vec<PathBuf>,
    open: Option<std::io::BufReader<std::fs::File>>,
}

impl Read for ChunkedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.header.read(buffer)?;
        if read > 0 || buffer.is_empty() {
            return Ok(read);
        }
        loop {
            if let Some(open) = self.open.as_mut() {
                let read = open.read(buffer)?;
                if read > 0 {
                    return Ok(read);
                }
                self.open = None;
            }
            let Some(next) = self.remaining.pop() else {
                return Ok(0);
            };
            self.open = Some(std::io::BufReader::with_capacity(
                READ_BUFFER_BYTES,
                std::fs::File::open(next)?,
            ));
        }
    }
}
