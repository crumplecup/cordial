//! Repair only managed arguments, using locations retained in the shared IR.

use super::super::present::PresentInstrument;
use super::super::types::{InstrumentLevel, InstrumentRecipe};
use super::{
    instrument::GapApplyOutcome,
    verifier_policy::{TracingApplyPolicy, gate_predicate},
};
use crate::ir::{SourceAttribute, SourceAttributeArguments, SourceByteRange};

type Edit = (std::ops::Range<usize>, String);

#[tracing::instrument(level = "debug", skip(lines, source, attr, recipe, policy))]
pub(super) fn repair(
    lines: &mut Vec<String>,
    source: &str,
    attr: &SourceAttribute,
    recipe: &InstrumentRecipe,
    policy: &TracingApplyPolicy,
) -> GapApplyOutcome {
    let Some(syntax) = attr.instrument_syntax() else {
        return GapApplyOutcome::Unresolved;
    };
    let present = PresentInstrument::from_syntax(syntax);
    if present.unresolved() {
        return GapApplyOutcome::Unresolved;
    }
    let mut edits = Vec::new();
    let mut additions = Vec::new();
    if let Some(args) = syntax.arguments() {
        // Duplicate managed keys cannot be repaired by selecting an arbitrary one.
        for key in ["level", "skip", "skip_all", "fields", "err", "ret"] {
            if args
                .entries()
                .iter()
                .filter(|arg| arg.name() == key)
                .count()
                > 1
            {
                return GapApplyOutcome::Unresolved;
            }
        }
        let existing_level = args.entries().iter().find(|arg| arg.name() == "level");
        let Some(level) = present.level() else {
            return GapApplyOutcome::Unresolved;
        };
        if level > recipe.level() {
            let replacement = format!("\"{}\"", recipe.level());
            if let Some(level) = existing_level {
                let Some(range) = level.value_range() else {
                    return GapApplyOutcome::Unresolved;
                };
                edits.push((range.start()..range.end(), replacement));
            } else {
                additions.push(format!("level = {replacement}"));
            }
        }
        if !args.entries().iter().any(|arg| arg.name() == "skip_all") {
            if !merge_list(
                args,
                "skip",
                recipe
                    .skip()
                    .iter()
                    .filter(|name| !list_has(args, "skip", name))
                    .cloned()
                    .collect(),
                &mut additions,
                &mut edits,
            ) {
                return GapApplyOutcome::Unresolved;
            }
        }
        if !merge_list(
            args,
            "fields",
            recipe
                .fields()
                .iter()
                .filter(|name| !list_has(args, "fields", name))
                .map(|name| format!("{name} = {name}"))
                .collect(),
            &mut additions,
            &mut edits,
        ) {
            return GapApplyOutcome::Unresolved;
        }
        if let Some(required) = recipe.err() {
            match args.entries().iter().find(|arg| arg.name() == "err") {
                None => additions.push(format!("err(level = \"{required}\")")),
                Some(arg) if present.err_level().is_some_and(|actual| actual < required) => {
                    let Some(range) = arg
                        .arguments()
                        .as_ref()
                        .and_then(|list| list.entries().iter().find(|arg| arg.name() == "level"))
                        .and_then(|arg| arg.value_range().as_ref())
                    else {
                        return GapApplyOutcome::Unresolved;
                    };
                    edits.push((range.start()..range.end(), format!("\"{required}\"")));
                }
                Some(_) => {}
            }
        }
        append(args, additions, &mut edits);
    } else {
        // A bare annotation has no unrelated arguments to overwrite.
        if InstrumentLevel::Info > recipe.level() {
            additions.push(format!("level = \"{}\"", recipe.level()));
        }
        if !recipe.skip().is_empty() {
            additions.push(format!("skip({})", recipe.skip().join(", ")));
        }
        if !recipe.fields().is_empty() {
            additions.push(format!(
                "fields({})",
                recipe
                    .fields()
                    .iter()
                    .map(|name| format!("{name} = {name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(level) = recipe.err() {
            additions.push(format!("err(level = \"{level}\")"));
        }
        if !additions.is_empty() {
            edits.push((
                syntax.meta_range().end()..syntax.meta_range().end(),
                format!("({})", additions.join(", ")),
            ));
        }
    }
    if let TracingApplyPolicy::Gated(cfgs) = policy {
        if let Some(existing) = syntax.gate_predicate() {
            if existing != &format!("not({})", gate_predicate(cfgs)) {
                return GapApplyOutcome::Unresolved;
            }
        } else {
            // Gating a short import can make that import unused in prover builds.
            // Leave it open until import ownership can be established in the IR.
            if attr.path() == "instrument" {
                return GapApplyOutcome::Unresolved;
            }
            edits.push((
                syntax.meta_range().start()..syntax.meta_range().start(),
                format!("cfg_attr(not({}), ", gate_predicate(cfgs)),
            ));
            edits.push((
                syntax.meta_range().end()..syntax.meta_range().end(),
                ")".into(),
            ));
        }
    }
    if edits.is_empty() {
        return GapApplyOutcome::AlreadyInstrumented;
    }
    replace_located_block(lines, source, syntax.meta_range(), edits)
}

#[tracing::instrument(level = "debug", skip(args, values, additions, edits))]
fn merge_list(
    args: &SourceAttributeArguments,
    key: &str,
    values: Vec<String>,
    additions: &mut Vec<String>,
    edits: &mut Vec<Edit>,
) -> bool {
    if values.is_empty() {
        return true;
    }
    match args.entries().iter().find(|arg| arg.name() == key) {
        Some(arg) => {
            let Some(list) = arg.arguments() else {
                return false;
            };
            append(list, values, edits);
        }
        None => additions.push(format!("{key}({})", values.join(", "))),
    }
    true
}

#[tracing::instrument(level = "trace", skip(args))]
fn list_has(args: &SourceAttributeArguments, key: &str, name: &str) -> bool {
    args.entries()
        .iter()
        .find(|arg| arg.name() == key)
        .and_then(|arg| arg.arguments().as_ref())
        .is_some_and(|list| list.entries().iter().any(|entry| entry.name() == name))
}

#[tracing::instrument(level = "trace", skip(args, values, edits))]
fn append(args: &SourceAttributeArguments, values: Vec<String>, edits: &mut Vec<Edit>) {
    if values.is_empty() {
        return;
    }
    let prefix = if args.entries().is_empty() {
        ""
    } else if args.trailing_comma() {
        " "
    } else {
        ", "
    };
    let end = args.range().end();
    edits.push((end..end, format!("{prefix}{}", values.join(", "))));
}

#[tracing::instrument(level = "debug", skip(lines, source, range, edits))]
fn replace_located_block(
    lines: &mut Vec<String>,
    source: &str,
    range: &SourceByteRange,
    mut edits: Vec<Edit>,
) -> GapApplyOutcome {
    let Some(before) = source.get(..range.start()) else {
        return GapApplyOutcome::Unresolved;
    };
    let Some(after) = source.get(range.end()..) else {
        return GapApplyOutcome::Unresolved;
    };
    let start = before.rfind('\n').map_or(0, |index| index + 1);
    let mut end = range.end() + after.find('\n').unwrap_or(after.len());
    if source.as_bytes().get(end.saturating_sub(1)) == Some(&b'\r') {
        end -= 1;
    }
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let last_line = first_line
        + source[start..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
    let Some(current) = lines.get(first_line..=last_line) else {
        return GapApplyOutcome::Unresolved;
    };
    if current.join("\n") != source[start..end].lines().collect::<Vec<_>>().join("\n") {
        return GapApplyOutcome::Unresolved;
    }
    edits.sort_by_key(|(range, _)| range.start);
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
        return GapApplyOutcome::Unresolved;
    }
    let mut block = source[start..end].to_string();
    for (range, replacement) in edits.into_iter().rev() {
        if range.start < start
            || range.end > end
            || !block.is_char_boundary(range.start - start)
            || !block.is_char_boundary(range.end - start)
        {
            return GapApplyOutcome::Unresolved;
        }
        block.replace_range(range.start - start..range.end - start, &replacement);
    }
    lines.splice(first_line..=last_line, block.lines().map(str::to_string));
    GapApplyOutcome::Applied
}

/// Remove only an attribute whose entire syntax is owned by instrumentation.
#[tracing::instrument(level = "debug", skip(lines, source, attribute))]
pub(super) fn strip(
    lines: &mut Vec<String>,
    source: &str,
    attribute: &SourceAttribute,
) -> GapApplyOutcome {
    if attribute.instrument_syntax().is_none() {
        return GapApplyOutcome::Unresolved;
    }
    let Some(range) = attribute.range() else {
        return GapApplyOutcome::Unresolved;
    };
    replace_located_block(
        lines,
        source,
        range,
        vec![(range.start()..range.end(), String::new())],
    )
}
