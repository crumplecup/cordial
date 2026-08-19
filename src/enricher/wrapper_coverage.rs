use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{ATTR_QUALIFIED_PATH, BasicQuery, IrMut, NodeKind};
use crate::loader::LoadView;
use crate::rustdoc::lookup_wrapper_coverage;
use crate::session::SessionView;

use tracing::instrument;
/// Attaches wrapper coverage attrs on type nodes from the elicitation hub map.
#[derive(Debug, Default, Clone, Copy)]
pub struct WrapperCoverageEnricher;

impl WrapperCoverageEnricher {
    pub const ID: &'static str = "wrapper-coverage";

    pub const ATTR_WRAPPER_COVERAGE: &'static str = "wrapper_coverage";
}

impl IrEnricher for WrapperCoverageEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        6
    }

    #[instrument(level = "trace", skip(self))]
    fn required_loader(&self) -> &str {
        crate::RustdocLoader::ID
    }

    #[instrument(level = "trace", skip(self, ir, _load, _session))]
    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        _load: &dyn LoadView,
        _session: &dyn SessionView,
    ) -> CordialResult<()> {
        let Some(map) = ir.workspace_wrapper_coverage() else {
            return Ok(());
        };
        if map.is_empty() {
            return Ok(());
        }

        let type_nodes: Vec<_> = ir
            .nodes_matching(&BasicQuery::all_nodes())
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Item(_)))
            .filter_map(|node| {
                let path = node.attr(ATTR_QUALIFIED_PATH)?.as_str()?.to_string();
                Some((node.id, path))
            })
            .collect();

        let updates: Vec<_> = type_nodes
            .into_iter()
            .filter_map(|(node_id, type_path)| {
                lookup_wrapper_coverage(map, &type_path).map(|wrappers| (node_id, wrappers.clone()))
            })
            .collect();

        for (node_id, wrappers) in updates {
            ir.set_attr(
                node_id,
                Self::ATTR_WRAPPER_COVERAGE,
                serde_json::to_value(wrappers)?,
            )?;
        }
        Ok(())
    }
}
