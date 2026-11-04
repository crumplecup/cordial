//! Interpret located instrument arguments retained in the shared IR.

use crate::CordialResult;
use crate::ir::{EdgeKind, IrView, NodeId, SourceAttributeArguments, SourceInstrumentSyntax};

use super::types::InstrumentLevel;
use tracing::instrument;

/// Existing annotation policy; unknown levels remain explicit.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct PresentInstrument {
    #[getter(copy)]
    level: Option<InstrumentLevel>,
    skip: Vec<String>,
    #[getter(copy)]
    skip_all: bool,
    fields: Vec<String>,
    #[getter(copy)]
    err: bool,
    #[getter(copy)]
    err_level: Option<InstrumentLevel>,
    #[getter(copy)]
    unresolved: bool,
}

impl PresentInstrument {
    /// Interpret shared argument facts, without parsing rendered attribute text.
    #[instrument(level = "debug", skip(syntax))]
    pub(crate) fn from_syntax(syntax: &SourceInstrumentSyntax) -> Self {
        let args = syntax.arguments().as_ref();
        let err = args.and_then(|args| args.entries().iter().find(|arg| arg.name() == "err"));
        let level = argument_level(args, InstrumentLevel::Info);
        let err_level =
            err.and_then(|arg| argument_level(arg.arguments().as_ref(), InstrumentLevel::Error));
        Self {
            level,
            skip: list_names(args, "skip"),
            skip_all: args
                .is_some_and(|args| args.entries().iter().any(|arg| arg.name() == "skip_all")),
            fields: list_names(args, "fields"),
            err: err.is_some(),
            err_level,
            unresolved: level.is_none()
                || (err.is_some() && err_level.is_none())
                || args.is_some_and(ambiguous_arguments),
        }
    }
}

/// Literal severity only. Constants need resolved value evidence, even when
/// their spelling resembles a tracing constant.
#[instrument(level = "trace")]
pub(super) fn parse_level(value: &str) -> Option<InstrumentLevel> {
    let literal = value.strip_prefix('"')?.strip_suffix('"')?;
    InstrumentLevel::from_attr(&literal.to_ascii_lowercase())
}

#[instrument(level = "trace", skip(args, default))]
fn argument_level(
    args: Option<&SourceAttributeArguments>,
    default: InstrumentLevel,
) -> Option<InstrumentLevel> {
    let Some(args) = args else {
        return Some(default);
    };
    let mut levels = args.entries().iter().filter(|arg| arg.name() == "level");
    let Some(level) = levels.next() else {
        return Some(default);
    };
    if levels.next().is_some() {
        return None;
    }
    level.value().as_deref().and_then(parse_level)
}

#[instrument(level = "trace", skip(args))]
fn ambiguous_arguments(args: &SourceAttributeArguments) -> bool {
    ["level", "skip", "skip_all", "fields", "err", "ret"]
        .iter()
        .any(|key| {
            args.entries()
                .iter()
                .filter(|arg| arg.name() == *key)
                .count()
                > 1
        })
}

#[instrument(level = "trace", skip(args))]
fn list_names(args: Option<&SourceAttributeArguments>, key: &str) -> Vec<String> {
    args.and_then(|args| args.entries().iter().find(|arg| arg.name() == key))
        .and_then(|arg| arg.arguments().as_ref())
        .map(|args| {
            args.entries()
                .iter()
                .map(|arg| arg.name().clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Load typed attribute evidence. A missing snapshot is a structural failure;
/// unsupported syntax remains visible as an unresolved policy finding.
#[instrument(level = "debug", skip(ir, node_id), err(level = "warn"))]
pub fn present_instrument(
    ir: &dyn IrView,
    node_id: NodeId,
) -> CordialResult<Option<PresentInstrument>> {
    let mut present = None;
    for child_id in ir.children(node_id, EdgeKind::HasAttr) {
        let attr = ir.source_attribute(child_id)?.ok_or_else(|| {
            crate::CordialError::workspace_source_facts_missing("typed source attribute")
        })?;
        if !attr.instrument() {
            continue;
        }
        let mut next = match attr.instrument_syntax() {
            Some(syntax) => PresentInstrument::from_syntax(syntax),
            None => PresentInstrument {
                level: None,
                skip: Vec::new(),
                skip_all: false,
                fields: Vec::new(),
                err: false,
                err_level: None,
                unresolved: true,
            },
        };
        if present.is_some() {
            next.unresolved = true;
        }
        present = Some(next);
    }
    Ok(present)
}
