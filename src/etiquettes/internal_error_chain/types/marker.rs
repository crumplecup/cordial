use crate::objects::{IrAnchor, Marker, SourceSpan};

#[derive(Debug, Clone)]
pub struct InternalErrorChainMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for InternalErrorChainMarker {
    fn probe(&self) -> &str {
        "internal-error-chain"
    }

    fn label(&self) -> &str {
        "internal-error-chain"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}
