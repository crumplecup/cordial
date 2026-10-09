//! Built-in After rows, kept in three area files and joined in order.
//! Validated in `const`.
//!
//! Every constant in the area files is named by its full path rather than
//! imported: each one exists only under the features of the rows that use
//! it, so an import would need the same `cfg` and double the file's length.

mod code;
mod polish;
mod structure;

use super::ids::KNOWN_IDS;
use crate::etiquette::order::{LintConstraint, OrderExplain, table_is_valid};

const PLACEHOLDER: LintConstraint = LintConstraint::new("", &[], OrderExplain::new("", ""));

pub(super) const CONSTRAINTS: &[LintConstraint] = &{
    let mut rows = [PLACEHOLDER; code::ROWS.len() + structure::ROWS.len() + polish::ROWS.len()];
    let mut index = 0;
    index = append_rows(&mut rows, index, code::ROWS);
    index = append_rows(&mut rows, index, structure::ROWS);
    append_rows(&mut rows, index, polish::ROWS);
    rows
};

const fn append_rows(dest: &mut [LintConstraint], start: usize, src: &[LintConstraint]) -> usize {
    let mut offset = 0;
    while offset < src.len() {
        dest[start + offset] = src[offset];
        offset += 1;
    }
    start + src.len()
}

const _: [(); 0] = [(); (!table_is_valid(KNOWN_IDS, CONSTRAINTS)) as usize];
