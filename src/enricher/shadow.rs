use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{BasicQuery, EdgeKind, IrMut, IrView};
use crate::loader::LoadView;
use crate::session::SessionView;

use tracing::instrument;
/// One upstream → shadow item mapping.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ShadowMapEntry {
    pub target: String,
    pub shadow: String,
}

/// Adds [`EdgeKind::Mirrors`] edges from the shadow roster or `{name}Shadow` pairs in the IR.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowLinkEnricher;

impl ShadowLinkEnricher {
    pub const ID: &'static str = "shadow-link";
}

impl IrEnricher for ShadowLinkEnricher {
    fn id(&self) -> &str {
        Self::ID
    }

    fn priority(&self) -> u8 {
        5
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        _load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()> {
        let entries = resolve_shadow_entries(session, ir)?;
        for entry in entries {
            let Some(target) = ir.node_by_path(&entry.target) else {
                continue;
            };
            let Some(shadow) = ir.node_by_path(&entry.shadow) else {
                continue;
            };
            ir.insert_edge(target, shadow, EdgeKind::Mirrors)?;
            ir.set_attr(
                target,
                "shadow_path",
                serde_json::Value::String(entry.shadow),
            )?;
        }
        Ok(())
    }
}

/// Resolve shadow item pairs: optional `shadow-map.json`, then same-crate `{name}Shadow` discovery.
#[instrument(level = "debug", skip(session, ir), err(level = "warn"))]
pub fn resolve_shadow_entries(
    session: &dyn SessionView,
    ir: &dyn IrView,
) -> CordialResult<Vec<ShadowMapEntry>> {
    let map_path = session.project_root().join("shadow-map.json");
    if map_path.is_file() {
        return load_shadow_map(&map_path);
    }
    Ok(discover_same_crate_shadow_pairs(ir))
}

/// Pair public items with a sibling whose last path segment adds the `Shadow` suffix.
#[instrument(level = "debug", skip(ir))]
pub fn discover_same_crate_shadow_pairs(ir: &dyn IrView) -> Vec<ShadowMapEntry> {
    static ALL_NODES: BasicQuery = BasicQuery {
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
        attr_key: None,
        attr_value: None,
    };

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for node in ir.nodes_matching(&ALL_NODES) {
        let Some(target_path) = node.attr("qualified_path").and_then(|value| value.as_str()) else {
            continue;
        };
        if target_path.ends_with("Shadow") {
            continue;
        }
        let Some(shadow_path) = shadow_path_for(target_path) else {
            continue;
        };
        if ir.node_by_path(&shadow_path).is_none() {
            continue;
        }
        if seen.insert((target_path.to_string(), shadow_path.clone())) {
            entries.push(ShadowMapEntry {
                target: target_path.to_string(),
                shadow: shadow_path,
            });
        }
    }
    entries
}

fn shadow_path_for(target_path: &str) -> Option<String> {
    let (prefix, name) = target_path.rsplit_once("::")?;
    if name.ends_with("Shadow") {
        return None;
    }
    Some(format!("{prefix}::{name}Shadow"))
}

#[instrument(level = "info", skip(path), err(level = "warn"))]
pub fn load_shadow_map(path: &Path) -> CordialResult<Vec<ShadowMapEntry>> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use miette::{IntoDiagnostic, WrapErr};

    use crate::RustdocLoadView;
    use crate::ir::CrateIr;
    use crate::rustdoc::parse_rustdoc_json;
    use crate::rustdoc::{demo_shadow_crate, write_rustdoc_crate_json};

    use super::*;

    #[test]
    fn discovers_widget_shadow_pair_without_map_file() -> miette::Result<()> {
        let dir = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
        let json = dir.path().join("demo.json");
        write_rustdoc_crate_json(&json, &demo_shadow_crate())
            .into_diagnostic()
            .wrap_err("write json")?;
        let inventory = parse_rustdoc_json(&json, "demo")
            .into_diagnostic()
            .wrap_err("parse")?;
        let view = RustdocLoadView::from_inventory(inventory);
        let mut ir = CrateIr::new("demo");
        view.populate_ir(&mut ir)
            .into_diagnostic()
            .wrap_err("populate")?;

        let entries = discover_same_crate_shadow_pairs(&ir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].target, "demo::Widget");
        assert_eq!(entries[0].shadow, "demo::WidgetShadow");
        Ok(())
    }
}
