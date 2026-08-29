use super::anchor::IrAnchor;
use super::span::SourceSpan;

/// An observation attached to an IR node by a probe.
pub trait Marker: Send + Sync {
    /// Id of the probe that emitted this marker.
    fn probe(&self) -> &str;
    /// Short marker label.
    fn label(&self) -> &str;
    /// IR location this item is attached to.
    fn anchor(&self) -> &dyn IrAnchor;
    /// Optional source span, when the loader recorded one.
    fn span(&self) -> Option<&dyn SourceSpan>;

    /// Optional structured payload for assessors (`type_path`, `gap_kind`, …).
    fn field(&self, _key: &str) -> Option<&str> {
        None
    }
}
