//! Syntactic parsing of a type as written in an evidence name or config.

use tracing::instrument;

/// A type's syntax: a path and its generic arguments, nothing resolved.
///
/// Non-path types (references, tuples, slices, `dyn`/`impl`/`fn`) are
/// opaque: the whole text is the `path` and there are no arguments.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct TypeText {
    /// The path as written, e.g. `chrono::DateTime` or `Utc`.
    path: String,
    /// Parsed generic arguments; lifetimes are dropped.
    args: Vec<TypeText>,
}

/// Canonical spelling of a type as written in an evidence name. Macros
/// render types with `stringify!`, so the same type can arrive with
/// different spacing (`A < B >` vs `A<B>`). Whitespace next to punctuation
/// is dropped and any other run collapses to one space, which keeps
/// `dyn Trait` and `&mut T` intact.
#[instrument(level = "trace")]
pub fn normalize_type_text(text: &str) -> String {
    const PUNCT: &str = ":<>,()[];&*";
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        let after_punct = out
            .chars()
            .next_back()
            .is_some_and(|prev| PUNCT.contains(prev));
        if pending_space && !after_punct && !PUNCT.contains(ch) {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }
    out
}

/// Parse normalized type text into a [`TypeText`]. `None` when the angle
/// brackets are unbalanced.
#[instrument(level = "debug")]
pub fn parse_type_text(text: &str) -> Option<TypeText> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if is_opaque_form(text) {
        return Some(TypeText::new(text.to_string(), Vec::new()));
    }
    let Some(open) = text.find('<') else {
        return Some(TypeText::new(text.to_string(), Vec::new()));
    };
    let inner = text[open + 1..].strip_suffix('>')?;
    let mut args = Vec::new();
    for piece in split_top_level(inner)? {
        if piece.starts_with('\'') {
            continue;
        }
        args.push(parse_type_text(piece)?);
    }
    Some(TypeText::new(text[..open].trim().to_string(), args))
}

#[instrument(level = "trace")]
fn is_opaque_form(text: &str) -> bool {
    text.starts_with(['&', '*', '(', '[', '\''])
        || ["dyn ", "impl ", "fn(", "fn ("]
            .iter()
            .any(|prefix| text.starts_with(prefix))
}

/// Split on commas at angle/paren/bracket depth zero; `None` if unbalanced.
#[instrument(level = "trace")]
fn split_top_level(text: &str) -> Option<Vec<&str>> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut pieces = Vec::new();
    let mut previous = '\0';
    for (index, ch) in text.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            // `->` in a fn type is not a closing angle bracket.
            '>' if previous == '-' => {}
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                pieces.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
        previous = ch;
        if depth < 0 {
            return None;
        }
    }
    if depth != 0 {
        return None;
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        pieces.push(last);
    }
    Some(pieces)
}

impl std::fmt::Display for TypeText {
    /// The canonical text form: `path<arg, arg>`, no stringify spacing.
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.path)?;
        if self.args.is_empty() {
            return Ok(());
        }
        f.write_str("<")?;
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{arg}")?;
        }
        f.write_str(">")
    }
}
