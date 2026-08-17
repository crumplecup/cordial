use super::anchor::IrAnchor;
use super::span::SourceSpan;

/// An observation attached to an IR node by a probe.
pub trait Marker: Send + Sync {
    fn probe(&self) -> &str;
    fn label(&self) -> &str;
    fn anchor(&self) -> &dyn IrAnchor;
    fn span(&self) -> Option<&dyn SourceSpan>;

    /// Optional structured payload for assessors (`type_path`, `gap_kind`, …).
    fn field(&self, _key: &str) -> Option<&str> {
        None
    }
}
