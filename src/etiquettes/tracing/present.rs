//! Parse the `#[instrument]` currently on a function from IR `HasAttr` nodes.

use crate::ir::{EdgeKind, IrView, NodeId};

use super::types::InstrumentLevel;

/// Arguments recorded on an existing `#[instrument]` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentInstrument {
    pub level: InstrumentLevel,
    pub skip: Vec<String>,
    pub skip_all: bool,
    pub fields: Vec<String>,
    pub err: bool,
    pub ret: bool,
}

impl Default for PresentInstrument {
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
#[tracing::instrument(skip(ir))]
pub fn present_instrument(ir: &dyn IrView, node_id: NodeId) -> Option<PresentInstrument> {
    for child_id in ir.children(node_id, EdgeKind::HasAttr) {
        let Some(child) = ir.node(child_id) else {
            continue;
        };
        let path = child
            .attr("attr_path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !is_instrument_path(path) {
            continue;
        }
        let meta = child
            .attr("meta")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return Some(parse_instrument_meta(meta));
    }
    None
}

fn is_instrument_path(path: &str) -> bool {
    path == "instrument" || path.ends_with("::instrument")
}

/// Parse `instrument` or `instrument(...)` meta rendered by the attribute enricher.
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

fn instrument_inner(meta: &str) -> Option<&str> {
    let rest = meta.strip_prefix("instrument")?;
    if rest.is_empty() {
        return Some("");
    }
    let rest = rest.trim();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.trim())
}

fn named_value<'a>(arg: &'a str, name: &str) -> Option<&'a str> {
    let rest = arg.strip_prefix(name)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    Some(rest)
}

fn ident_eq(arg: &str, name: &str) -> bool {
    arg == name
}

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

fn csv_idents(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

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
