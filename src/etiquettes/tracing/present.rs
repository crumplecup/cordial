//! Parse the `#[instrument]` currently on a function from IR `HasAttr` nodes.

use crate::ir::{EdgeKind, IrView, NodeId};

use super::types::InstrumentLevel;

use tracing::instrument;
/// Arguments recorded on an existing `#[instrument]` attribute.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct PresentInstrument {
    #[getter(copy)]
    level: InstrumentLevel,
    skip: Vec<String>,
    #[getter(copy)]
    skip_all: bool,
    fields: Vec<String>,
    #[getter(copy)]
    err: bool,
    #[getter(copy)]
    ret: bool,
}

impl Default for PresentInstrument {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            level: InstrumentLevel::Info,
            skip: Vec::new(),
            skip_all: false,
            fields: Vec::new(),
            err: false,
            ret: false,
        }
    }
}

/// Load present instrument args from `HasAttr` children, if any.
#[instrument(level = "debug", skip(ir, node_id))]
pub fn present_instrument(ir: &dyn IrView, node_id: NodeId) -> Option<PresentInstrument> {
    for child_id in ir.children(node_id, EdgeKind::HasAttr) {
        let Some(child) = ir.node(child_id) else {
            continue;
        };
        let path = child
            .attr("attr_path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let meta = child
            .attr("meta")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if let Some(present) = present_from_attr(path, meta) {
            return Some(present);
        }
    }
    None
}

#[instrument(level = "trace", skip(path, meta))]
fn present_from_attr(path: &str, meta: &str) -> Option<PresentInstrument> {
    if is_instrument_path(path) {
        return Some(parse_instrument_meta(meta));
    }
    if path != "cfg_attr" {
        return None;
    }
    let inner = cfg_attr_inner_attr(meta)?;
    let compact: String = inner.split_whitespace().collect();
    let rest = compact.strip_prefix("::").unwrap_or(compact.as_str());
    let rest = rest.strip_prefix("tracing::").unwrap_or(rest);
    rest.starts_with("instrument")
        .then(|| parse_instrument_meta(rest))
}

#[instrument(level = "trace", skip(path), ret)]
fn is_instrument_path(path: &str) -> bool {
    path == "instrument" || path.ends_with("::instrument")
}

/// Second argument of `cfg_attr(<predicate>, <inner>)` as stored by the
/// attribute enricher (`cfg_attr(not(kani), tracing::instrument(...))`).
#[instrument(level = "trace")]
fn cfg_attr_inner_attr(meta: &str) -> Option<&str> {
    let rest = meta.trim().strip_prefix("cfg_attr(")?.strip_suffix(')')?;
    let mut depth: u32 = 0;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(rest[idx + 1..].trim()),
            _ => {}
        }
    }
    None
}

/// Parse `instrument` or `instrument(...)` meta rendered by the attribute enricher.
#[instrument(level = "debug")]
pub fn parse_instrument_meta(meta: &str) -> PresentInstrument {
    let mut present = PresentInstrument::default();
    let trimmed = meta.trim();
    let Some(inner) = instrument_inner(trimmed) else {
        return present;
    };
    if inner.is_empty() {
        return present;
    }
    for arg in split_top_level(inner) {
        let arg = arg.trim();
        if arg.is_empty() {
            continue;
        }
        if let Some(level) = named_value(arg, "level") {
            if let Some(parsed) = parse_level(level) {
                present.level = parsed;
            }
            continue;
        }
        if ident_eq(arg, "skip_all") || grouped_args(arg, "skip_all").is_some() {
            present.skip_all = true;
            continue;
        }
        if let Some(names) = grouped_args(arg, "skip") {
            present.skip = csv_idents(names);
            continue;
        }
        if let Some(fields) = grouped_args(arg, "fields") {
            present.fields = field_names(fields);
            continue;
        }
        if ident_eq(arg, "err") || grouped_args(arg, "err").is_some() {
            present.err = true;
            continue;
        }
        if ident_eq(arg, "ret") || grouped_args(arg, "ret").is_some() {
            present.ret = true;
        }
    }
    present
}

#[instrument(level = "debug")]
fn instrument_inner(meta: &str) -> Option<&str> {
    let rest = meta.strip_prefix("instrument")?;
    if rest.is_empty() {
        return Some("");
    }
    let rest = rest.trim();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.trim())
}

#[instrument(level = "debug")]
fn named_value<'a>(arg: &'a str, name: &str) -> Option<&'a str> {
    let rest = arg.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    Some(rest)
}

#[instrument(level = "debug")]
fn ident_eq(arg: &str, name: &str) -> bool {
    arg == name
}

#[instrument(level = "debug")]
fn grouped_args<'a>(arg: &'a str, name: &str) -> Option<&'a str> {
    let rest = arg.strip_prefix(name)?;
    if !rest.is_empty() && !rest.starts_with(|ch: char| ch == '(' || ch.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix('(')?.strip_suffix(')').map(str::trim)
}

#[instrument(level = "debug")]
fn parse_level(value: &str) -> Option<InstrumentLevel> {
    let token = value
        .trim()
        .trim_matches('"')
        .rsplit("::")
        .next()
        .unwrap_or(value)
        .trim();
    InstrumentLevel::from_attr(&token.to_ascii_lowercase())
}

#[instrument(level = "debug")]
fn split_top_level(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth: u32 = 0;
    for (idx, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < input.len() {
        parts.push(&input[start..]);
    }
    parts
}

#[instrument(level = "debug")]
fn csv_idents(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[instrument(level = "debug")]
fn field_names(inner: &str) -> Vec<String> {
    split_top_level(inner)
        .into_iter()
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let name = part.split('=').next().unwrap_or(part).trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}
