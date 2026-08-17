use crate::NamedRunFilter;
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{BasicQuery, IrMut, NodeKind};
use crate::loader::LoadView;
use crate::proof_harness::{load_workspace_proof_harness, test_status_for_type_path};
use crate::session::SessionView;

/// Attaches proof harness test status attrs on type nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofHarnessEnricher;

impl ProofHarnessEnricher {
    pub const ID: &'static str = "proof-harness";

    pub const ATTR_PROOF_TEST: &'static str = "proof_test";
    pub const ATTR_COMPOSITION_TEST: &'static str = "composition_test";
}

impl IrEnricher for ProofHarnessEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn priority(&self) -> u8 {
        7
    }

    fn required_loader(&self) -> &str {
        crate::RustdocLoader::ID
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        _load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()> {
        let filter = NamedRunFilter::all_etiquettes();
        let harness = load_workspace_proof_harness(session.project_root(), &filter)?;

        let type_nodes: Vec<_> = ir
            .nodes_matching(&BasicQuery::all_nodes())
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Item(_)))
            .filter_map(|node| {
                let path = node.attr("qualified_path")?.as_str()?.to_string();
                Some((node.id, path))
            })
            .collect();

        for (node_id, type_path) in type_nodes {
            let has_factory_impl = false;
            let (proof_test, composition_test) =
                test_status_for_type_path(&type_path, has_factory_impl, &harness);
            ir.set_attr(
                node_id,
                Self::ATTR_PROOF_TEST,
                serde_json::Value::String(proof_test.display()),
            )?;
            ir.set_attr(
                node_id,
                Self::ATTR_COMPOSITION_TEST,
                serde_json::Value::String(composition_test.display()),
            )?;
        }
        Ok(())
    }
}
