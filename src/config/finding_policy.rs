//! Resolve finding policy from collected locations, without reading source.

use crate::ir::IrView;
use crate::objects::{Finding, MapFindingSink, SourceSpan};

use super::{CordialConfig, LintLocation};

impl CordialConfig {
    /// Apply shared rule policy to a finding using its emitted or anchored file.
    /// An absent location permits only global controls to affect the finding.
    #[tracing::instrument(level = "trace", skip(self, finding, location, ir))]
    pub fn finding_enabled(
        &self,
        finding: &dyn Finding,
        location: LintLocation<'_>,
        ir: Option<&dyn IrView>,
    ) -> bool {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let node = ir.and_then(|ir| ir.node(finding.anchor().node_id()));
        let file = sink
            .fields()
            .iter()
            .find(|(key, value)| key == "file" && !value.is_empty())
            .map(|(_, value)| std::path::Path::new(value))
            .or_else(|| {
                node.as_ref()
                    .and_then(|node| node.weight().span().as_ref())
                    .map(SourceSpan::file)
            })
            .or_else(|| {
                node.as_ref()
                    .and_then(|node| node.attr("file"))
                    .and_then(|value| value.as_str())
                    .map(std::path::Path::new)
            })
            .or(location.file())
            .filter(|path| !path.as_os_str().is_empty());
        let location = LintLocation::new(
            location.workspace_root(),
            location.crate_root(),
            location.crate_name(),
            file,
        );
        self.lint_enabled(
            finding.rule().configuration_key(),
            finding.rule().id(),
            location,
        )
    }
}
