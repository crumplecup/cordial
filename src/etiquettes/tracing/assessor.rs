use std::path::PathBuf;

use crate::error::CordialResult;
use crate::hooks::Assessor;
use crate::ir::IrView;
use crate::objects::{Disposition, FileSpan, Finding, Marker};
use crate::session::SessionView;

use super::delta::{DeltaContext, recipe_deltas};
use super::present::present_instrument;
use super::types::{
    FunctionComplexity, FunctionKind, FunctionRole, InstrumentLevel, InstrumentRecipe,
    MISSING_INSTRUMENT_LABEL, RECIPE_DELTA_LABEL, TracingFinding, TracingRule, TracingRuleKind,
    VisibilityLabel,
};

/// Converts missing-instrument and recipe-delta markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingAssessor;

impl TracingAssessor {
    pub const ID: &'static str = "tracing-assessor";
}

impl Assessor for TracingAssessor {
    fn id(&self) -> &str {
        Self::ID
    }

    fn consumes(&self) -> &[&str] {
        &[MISSING_INSTRUMENT_LABEL, RECIPE_DELTA_LABEL]
    }

    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let include_pub_super = crate::config::load_session_config(session)
            .tracing
            .include_pub_super;
        let mut findings = Vec::new();
        for marker in markers {
            let Some(parsed) = ParsedFn::from_marker(*marker, ir, session) else {
                continue;
            };
            if !should_report(&parsed.visibility, include_pub_super) {
                continue;
            }
            match marker.label() {
                MISSING_INSTRUMENT_LABEL => {
                    findings.push(parsed.into_finding(TracingRuleKind::MissingInstrument));
                }
                RECIPE_DELTA_LABEL => {
                    let present = present_instrument(ir, parsed.anchor.0).unwrap_or_default();
                    let kinds = recipe_deltas(
                        &parsed.recipe,
                        &present,
                        &DeltaContext {
                            role: parsed.role,
                            complexity: parsed.complexity,
                            qualified_path: &parsed.qualified_name,
                            param_names: &parsed.param_names,
                            has_error_path_event: parsed.has_error_path_event,
                        },
                    );
                    for kind in kinds {
                        findings.push(parsed.clone().into_finding(kind));
                    }
                }
                _ => {}
            }
        }
        Ok(findings)
    }
}

#[derive(Debug, Clone)]
struct ParsedFn {
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    qualified_name: String,
    kind: FunctionKind,
    role: FunctionRole,
    complexity: FunctionComplexity,
    recipe: InstrumentRecipe,
    visibility: VisibilityLabel,
    span: FileSpan,
    param_names: Vec<String>,
    has_error_path_event: bool,
}

impl ParsedFn {
    fn from_marker(
        marker: &dyn Marker,
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> Option<Self> {
        let node_id = marker.anchor().node_id();
        let node = ir.node(node_id)?;
        let attr = |key: &str| {
            node.attr(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
        };
        let qualified_name = attr("qualified_path");
        let qualified_name = if qualified_name.is_empty() {
            "?".to_string()
        } else {
            qualified_name.to_string()
        };
        let kind = parse_function_kind(attr("function_kind"));
        let visibility = parse_visibility(attr("visibility"));
        let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let file = node
            .attr("file")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| session.project_root().to_path_buf());
        let span = FileSpan::new(file, line, 1);
        let role = FunctionRole::from_attr(attr("function_role")).unwrap_or(FunctionRole::Other);
        let complexity = FunctionComplexity::from_attr(attr("function_complexity"))
            .unwrap_or(FunctionComplexity::Linear);
        let recipe = InstrumentRecipe {
            level: InstrumentLevel::from_attr(attr("recipe_level"))
                .unwrap_or(InstrumentLevel::Debug),
            skip: csv_list(attr("recipe_skip")),
            fields: csv_list(attr("recipe_fields")),
            err: InstrumentLevel::from_attr(attr("recipe_err")),
            ret: node
                .attr("recipe_ret")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        };
        Some(Self {
            anchor: crate::objects::NodeAnchor(node_id),
            crate_name: ir.crate_name().to_string(),
            qualified_name,
            kind,
            role,
            complexity,
            recipe,
            visibility,
            span,
            param_names: csv_list(attr("param_names")),
            has_error_path_event: node
                .attr("has_error_path_event")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        })
    }

    fn into_finding(self, kind: TracingRuleKind) -> Box<dyn Finding> {
        Box::new(TracingFinding {
            rule: TracingRule::new(kind),
            disposition: Disposition::Open,
            anchor: self.anchor,
            crate_name: self.crate_name,
            qualified_name: self.qualified_name,
            kind: self.kind,
            role: self.role,
            complexity: self.complexity,
            recipe: self.recipe,
            visibility: self.visibility,
            span: self.span,
        })
    }
}

fn csv_list(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn should_report(visibility: &VisibilityLabel, include_pub_super: bool) -> bool {
    matches!(
        visibility,
        VisibilityLabel::Public | VisibilityLabel::PubCrate
    ) || (include_pub_super && matches!(visibility, VisibilityLabel::PubSuper))
}

fn parse_function_kind(value: &str) -> FunctionKind {
    match value {
        "inherent" => FunctionKind::InherentMethod,
        "trait_impl" => FunctionKind::TraitImplMethod,
        _ => FunctionKind::Free,
    }
}

fn parse_visibility(value: &str) -> VisibilityLabel {
    match value {
        "pub" => VisibilityLabel::Public,
        "pub(crate)" => VisibilityLabel::PubCrate,
        "pub(super)" => VisibilityLabel::PubSuper,
        other if other.starts_with("pub(") => VisibilityLabel::PubInPath(
            other
                .trim_start_matches("pub(")
                .trim_end_matches(')')
                .to_string(),
        ),
        _ => VisibilityLabel::Private,
    }
}
