use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::loader::{LoadView, SourceLoadView};
use crate::objects::FileSpan;
use crate::session::SessionView;

use super::scan::{BranchingCache, scan_crate_visibility_with_cache};

/// Materializes visibility-path nodes in the IR graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilityInventoryEnricher;

impl VisibilityInventoryEnricher {
    pub const ID: &'static str = "visibility-inventory";
}

impl IrEnricher for VisibilityInventoryEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()> {
        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        let crate_root = member_crate_root(source, session);
        let thresholds = crate::config::load_session_config(session).visibility;
        let cache_path = session
            .store_root()
            .join("cache")
            .join(format!("{}-visibility-branching.json", ir.crate_name()));
        let cached = if thresholds.prefer_root {
            None
        } else {
            BranchingCache::load(&cache_path)
        };
        let (records, new_cache) =
            scan_crate_visibility_with_cache(&crate_root, thresholds, cached)?;
        if let Some(cache) = new_cache {
            cache.write(&cache_path)?;
        }

        for record in records {
            let file = if record.file.is_absolute() {
                record.file.clone()
            } else {
                crate_root.join(&record.file)
            };
            let span = FileSpan::new(file.clone(), record.line, 1);
            let node = ir.insert_node(
                NodeWeight::new(NodeKind::Expr)
                    .with_span(span)
                    .with_name(record.module_path.clone()),
            )?;
            ir.set_attr(
                node,
                "visibility_rule_id",
                serde_json::Value::String(record.rule_id.as_str().to_string()),
            )?;
            ir.set_attr(
                node,
                "module_path",
                serde_json::Value::String(record.module_path),
            )?;
            ir.set_attr(
                node,
                "file",
                serde_json::Value::String(file.display().to_string()),
            )?;
            ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
            ir.set_attr(
                node,
                "name_count",
                serde_json::Value::Number(record.name_count.into()),
            )?;
            ir.set_attr(
                node,
                "parent_vis",
                serde_json::Value::String(record.parent_vis),
            )?;
            ir.set_attr(
                node,
                "declared_vis",
                serde_json::Value::String(record.declared_vis),
            )?;
            ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
        }

        Ok(())
    }
}
