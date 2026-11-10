//! Types tracing can record without extra bounds.
//!
//! The per-signature decisions are made on the shared tree in
//! `ir::tracing_source::recordable`; this module owns the shared name list.

/// Types tracing can record without a `Debug` bound blowing up the build.
pub(crate) const RECORDABLE_IDENTS: &[&str] = &[
    "bool", "char", "str", "String", "OsStr", "OsString", "Path", "PathBuf", "u8", "u16", "u32",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "Duration",
    "Option", "Result", "Vec", "Box", "Arc", "Rc", "Cow",
];
