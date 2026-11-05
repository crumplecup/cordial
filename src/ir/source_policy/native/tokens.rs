//! Shared rendering of canonical syntax tokens.
pub(super) use crate::loader::database_token_text as render;

#[tracing::instrument(level = "trace")]
#[cfg(any(feature = "allows", feature = "cfg_hygiene"))]
pub(super) fn truncate(text: String, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text
    }
}
