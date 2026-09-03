use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::etiquettes::error_chain::{ErrorChainProbeId, ErrorChainRecord};
use crate::etiquettes::error_sites::ErrorSiteKind;
use crate::etiquettes::foreign_error_types::{
    ForeignErrorRecordKind, ForeignErrorTypeRecord, ForeignErrorTypeReport,
};
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{BasicQuery, IrMut, IrView, NodeKind};
use crate::loader::SourceLoadView;

use super::assess::{ErrorBridgeHint, build_foreign_error_attenuation_report_with_bridges};

use tracing::instrument;

trait IrViewRef {
    fn as_view(&self) -> &dyn IrView;
}

impl IrViewRef for &mut dyn IrMut {
    #[instrument(level = "trace", skip(self))]
    fn as_view(&self) -> &dyn IrView {
        *self as &dyn IrView
    }
}

/// Joins typed foreign error sites with chain probes and annotates matching IR nodes.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationInventoryEnricher;

impl ForeignErrorAttenuationInventoryEnricher {
    pub const ID: &'static str = "foreign-error-attenuation-inventory";
}

impl IrEnricher for ForeignErrorAttenuationInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        52
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let load = view.load;
        let session = view.session;

        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        let crate_root = member_crate_root(source, session);
        let foreign_report = foreign_report_from_ir(ir.as_view(), &crate_root)?;
        let chain_records = chain_records_from_ir(ir.as_view(), &crate_root)?;
        let bridges = error_bridges_from_ir(ir.as_view());
        let attenuation_report = build_foreign_error_attenuation_report_with_bridges(
            &foreign_report,
            &chain_records,
            &bridges,
        )?;
        let sites_by_key = index_typed_error_sites(ir.as_view(), &crate_root);

        for record in attenuation_report.findings() {
            let key = SiteKey {
                file: record.file().clone(),
                line: record.line(),
            };
            let Some(site_id) = sites_by_key.get(&key) else {
                continue;
            };
            ir.set_attr(
                *site_id,
                "foreign_error_attenuation",
                serde_json::Value::Bool(true),
            )?;
            ir.set_attr(
                *site_id,
                "handling_class",
                serde_json::Value::String(record.handling_class().to_string()),
            )?;
            ir.set_attr(
                *site_id,
                "resolution_id",
                serde_json::Value::String(record.resolution_id().to_string()),
            )?;
            ir.set_attr(
                *site_id,
                "foreign_error_type",
                serde_json::Value::String(record.foreign_error_type().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "inference_rule_id",
                serde_json::Value::String(record.inference_rule_id().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "confidence",
                serde_json::Value::String(record.confidence().to_string()),
            )?;
            ir.set_attr(
                *site_id,
                "site_kind",
                serde_json::Value::String(record.kind().as_attr().to_string()),
            )?;
            ir.set_attr(
                *site_id,
                "context",
                serde_json::Value::String(record.context().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "source_snippet",
                serde_json::Value::String(record.source_snippet().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "site_snippet",
                serde_json::Value::String(record.site_snippet().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "resolution",
                serde_json::Value::String(record.resolution().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "good_pattern",
                serde_json::Value::String(record.good_pattern().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "bad_pattern",
                serde_json::Value::String(record.bad_pattern().clone()),
            )?;
            ir.set_attr(
                *site_id,
                "file",
                serde_json::Value::String(crate_root.join(record.file()).display().to_string()),
            )?;
            ir.set_attr(
                *site_id,
                "line",
                serde_json::Value::Number(record.line().into()),
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SiteKey {
    file: PathBuf,
    line: u32,
}

#[instrument(level = "debug", skip(ir), err(level = "warn"))]
fn foreign_report_from_ir(
    ir: &dyn IrView,
    crate_root: &Path,
) -> CordialResult<ForeignErrorTypeReport> {
    let crate_name = ir.crate_name().to_string();
    let mut findings = Vec::new();
    for node in ir.nodes_matching(&BasicQuery::all_nodes()) {
        if !matches!(node.kind(), NodeKind::Expr) {
            continue;
        }
        if let Some(record) = typed_foreign_record(node, &crate_name, crate_root)? {
            findings.push(record);
        }
    }

    Ok(ForeignErrorTypeReport::new(crate_name, findings))
}

#[instrument(level = "debug", skip(node), err(level = "warn"))]
fn typed_foreign_record(
    node: crate::ir::NodeRef<'_>,
    crate_name: &str,
    crate_root: &Path,
) -> CordialResult<Option<ForeignErrorTypeRecord>> {
    let Some(record_kind) = node
        .attr("foreign_error_record_kind")
        .and_then(|value| value.as_str())
    else {
        return Ok(None);
    };
    let Some(record_kind) = ForeignErrorRecordKind::from_attr(record_kind) else {
        return Ok(None);
    };
    if record_kind != ForeignErrorRecordKind::Typed {
        return Ok(None);
    }

    let Some(kind) = node
        .attr("error_site_kind")
        .or_else(|| node.attr("site_kind"))
        .and_then(|value| value.as_str())
        .and_then(ErrorSiteKind::from_attr)
    else {
        return Ok(None);
    };
    let context = node
        .attr("context")
        .and_then(|value| value.as_str())
        .unwrap_or("<crate>")
        .to_string();
    let source_snippet = node
        .attr("source_snippet")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let site_snippet = node
        .attr("site_snippet")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let line = node
        .attr("line")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let Some(file_attr) = node.attr("file").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    let file = relative_file(file_attr, crate_root);
    let foreign_error_type = node
        .attr("foreign_error_type")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let rule_id = node
        .attr("inference_rule_id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = node
        .attr("confidence")
        .and_then(|value| value.as_str())
        .map(parse_confidence)
        .unwrap_or(crate::etiquettes::error_sites::ForeignTypeConfidence::High);
    let chain_break = node
        .attr("chain_break")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    Ok(Some(
        ForeignErrorTypeRecord::builder()
            .crate_name(crate_name.to_string())
            .foreign_error_type(foreign_error_type)
            .rule_id(rule_id)
            .confidence(confidence)
            .chain_break(chain_break)
            .kind(kind)
            .context(context)
            .file(file)
            .line(line)
            .source_snippet(source_snippet)
            .site_snippet(site_snippet)
            .build()?,
    ))
}

#[instrument(level = "debug", skip(ir), err(level = "warn"))]
fn chain_records_from_ir(
    ir: &dyn IrView,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    let mut records = Vec::new();
    for node in ir.nodes_matching(&BasicQuery::all_nodes()) {
        if !matches!(node.kind(), NodeKind::Expr) {
            continue;
        }
        let Some(rule_id) = node
            .attr("error_chain_rule_id")
            .and_then(|value| value.as_str())
            .and_then(ErrorChainProbeId::from_attr)
        else {
            continue;
        };
        let context = node
            .attr("context")
            .and_then(|value| value.as_str())
            .unwrap_or("<crate>")
            .to_string();
        let snippet = node
            .attr("snippet")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let line = node
            .attr("line")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        let Some(file_attr) = node.attr("file").and_then(|value| value.as_str()) else {
            continue;
        };
        let file = relative_file(file_attr, crate_root);
        let foreign_error_type = node
            .attr("foreign_error_type")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        records.push(
            ErrorChainRecord::builder()
                .rule_id(rule_id)
                .context(context)
                .file(file)
                .line(line)
                .snippet(snippet)
                .foreign_error_type(foreign_error_type)
                .build()?,
        );
    }
    Ok(records)
}

#[instrument(level = "debug", skip(ir))]
fn error_bridges_from_ir(ir: &dyn IrView) -> Vec<ErrorBridgeHint> {
    ir.nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter_map(|node| {
            let kind = node
                .attr("internal_error_record_kind")
                .and_then(|value| value.as_str())?;
            if kind != "type_graph" {
                return None;
            }
            let class = node.attr("node_class").and_then(|value| value.as_str())?;
            if class != "ERROR-CHAIN-FOREIGN-BRIDGE" {
                return None;
            }
            let foreign_type = node
                .attr("source_target")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())?
                .to_string();
            let constructor = node
                .attr("type_path")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())?
                .to_string();
            Some(ErrorBridgeHint::new(foreign_type, constructor))
        })
        .collect()
}

#[instrument(level = "debug", skip(ir))]
fn index_typed_error_sites(
    ir: &dyn IrView,
    crate_root: &Path,
) -> HashMap<SiteKey, crate::ir::NodeId> {
    let mut map = HashMap::new();
    for node in ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| matches!(node.kind(), NodeKind::Expr))
    {
        let Some(record_kind) = node
            .attr("foreign_error_record_kind")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if ForeignErrorRecordKind::from_attr(record_kind) != Some(ForeignErrorRecordKind::Typed) {
            continue;
        }
        let line = node
            .attr("line")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        let Some(file_attr) = node.attr("file").and_then(|value| value.as_str()) else {
            continue;
        };
        let file = relative_file(file_attr, crate_root);
        map.insert(SiteKey { file, line }, node.id);
    }
    map
}

#[instrument(level = "debug", skip(path))]
fn relative_file(path: &str, crate_root: &Path) -> PathBuf {
    let resolved = PathBuf::from(path);
    resolved
        .strip_prefix(crate_root)
        .map(Path::to_path_buf)
        .unwrap_or(resolved)
}

#[instrument(level = "debug")]
fn parse_confidence(value: &str) -> crate::etiquettes::error_sites::ForeignTypeConfidence {
    if value.contains("MEDIUM") {
        crate::etiquettes::error_sites::ForeignTypeConfidence::Medium
    } else {
        crate::etiquettes::error_sites::ForeignTypeConfidence::High
    }
}
