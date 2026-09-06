use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, NodeKind, NodeWeight};

use super::loader::{DependencyFreshnessLoadView, DependencyFreshnessLoader};

use tracing::instrument;

/// Materializes dependency freshness survey rows into IR plugin nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessSurveyEnricher;

impl DependencyFreshnessSurveyEnricher {
    /// Stable enricher id.
    pub const ID: &'static str = "dependency-freshness-survey";
    /// IR attribute identifying dependency survey nodes.
    pub const ATTR_NODE_KIND: &'static str = "dependency_freshness_node";
}

impl IrEnricher for DependencyFreshnessSurveyEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn required_loader(&self) -> &str {
        DependencyFreshnessLoader::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let Some(load) = view
            .load
            .as_any()
            .downcast_ref::<DependencyFreshnessLoadView>()
        else {
            return Ok(());
        };
        let root = view.ir.root()?;
        for record in load.records() {
            let node = view.ir.insert_node(
                NodeWeight::new(NodeKind::Plugin("dependency_freshness".to_string()))
                    .with_name(record.dependency_name().clone()),
            )?;
            view.ir.set_attr(
                node,
                Self::ATTR_NODE_KIND,
                serde_json::Value::String("dependency".to_string()),
            )?;
            view.ir.set_attr(
                node,
                "crate",
                serde_json::Value::String(record.crate_name().clone()),
            )?;
            view.ir.set_attr(
                node,
                "dependency_name",
                serde_json::Value::String(record.dependency_name().clone()),
            )?;
            view.ir.set_attr(
                node,
                "package_name",
                serde_json::Value::String(record.package_name().clone()),
            )?;
            view.ir.set_attr(
                node,
                "manifest_path",
                serde_json::Value::String(record.manifest_path().display().to_string()),
            )?;
            view.ir.set_attr(
                node,
                "line",
                serde_json::Value::Number((*record.line()).into()),
            )?;
            view.ir.set_attr(
                node,
                "section",
                serde_json::Value::String(record.section().to_string()),
            )?;
            view.ir.set_attr(
                node,
                "version_spec",
                serde_json::Value::String(record.version_spec().to_string()),
            )?;
            view.ir.set_attr(
                node,
                "source_kind",
                serde_json::Value::String(record.source_kind().to_string()),
            )?;
            view.ir.set_attr(
                node,
                "locked_versions",
                serde_json::Value::String(record.locked_versions_display()),
            )?;
            view.ir.set_attr(
                node,
                "available_versions",
                serde_json::Value::String(record.available_versions_display()),
            )?;
            view.ir.set_attr(
                node,
                "update_kinds",
                serde_json::Value::String(record.update_kinds_display()),
            )?;
            view.ir.set_attr(
                node,
                "dependency_freshness_rule_ids",
                serde_json::Value::String(record.freshness_rule_ids_display()),
            )?;
            view.ir.set_attr(
                node,
                "indicators",
                serde_json::Value::String(record.indicators_display()),
            )?;
            view.ir.insert_edge(root, node, EdgeKind::Contains)?;
        }
        Ok(())
    }
}
