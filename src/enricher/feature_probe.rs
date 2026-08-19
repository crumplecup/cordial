use crate::NamedRunFilter;
use crate::error::CordialResult;
use crate::feature_probe::load_crate_feature_probes;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{ATTR_QUALIFIED_PATH, BasicQuery, NodeKind};

use tracing::instrument;
/// Attaches per-type feature probe attrs from cached or live probe rustdoc.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureProbeEnricher;

impl FeatureProbeEnricher {
    pub const ID: &'static str = "feature-probe";

    pub const ATTR_CRATE: &'static str = "feature_probe_crate";
    pub const ATTR_CANDIDATE_FEATURES: &'static str = "feature_probe_candidate_unlock_features";
    pub const ATTR_PROBED_PREREQS: &'static str = "feature_probe_probed_prereqs";
}

impl IrEnricher for FeatureProbeEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        5
    }

    #[instrument(level = "trace", skip(self))]
    fn required_loader(&self) -> &str {
        crate::RustdocLoader::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let session = view.session;

        let filter = NamedRunFilter::all_etiquettes();
        let type_paths: Vec<String> = ir
            .nodes_matching(&BasicQuery::all_nodes())
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Item(_)))
            .filter_map(|node| {
                node.attr(ATTR_QUALIFIED_PATH)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .collect();

        let probes = load_crate_feature_probes(
            session.project_root(),
            session.store_root(),
            &filter,
            ir.crate_name(),
            &type_paths,
        )?;
        if probes.is_empty() {
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

        for (node_id, type_path) in type_nodes {
            let Some(probe) = probes.get(&type_path) else {
                continue;
            };
            ir.set_attr(
                node_id,
                Self::ATTR_CRATE,
                serde_json::Value::String(probe.feature_crate.clone()),
            )?;
            ir.set_attr(
                node_id,
                Self::ATTR_CANDIDATE_FEATURES,
                serde_json::to_value(&probe.candidate_unlock_features)?,
            )?;
            if let Some(prereqs) = &probe.probed_prereqs {
                ir.set_attr(
                    node_id,
                    Self::ATTR_PROBED_PREREQS,
                    serde_json::to_value(prereqs)?,
                )?;
            }
        }
        Ok(())
    }
}
