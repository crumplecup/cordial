//! Whether the id of a tracked struct still names it.

/// What became of the id of a tracked struct, found without reading the struct: a struct that is gone has
/// fields that cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Existence {
    /// The id names a struct that exists. Everything that is not a tracked struct is here: nothing else is
    /// deleted while a database is used.
    Exists,
    /// The query that made the struct ran again and did not make it again: it is deleted, and its slot waits to
    /// be reused.
    Deleted,
    /// The slot was reused for another struct: the id is an earlier generation of it.
    Replaced,
}
