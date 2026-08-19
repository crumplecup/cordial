use crate::objects::{IrAnchor, Marker, SourceSpan};

use tracing::instrument;
#[derive(Debug, Clone)]
pub struct InternalErrorChainMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for InternalErrorChainMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "internal-error-chain"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "internal-error-chain"
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self))]
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}
