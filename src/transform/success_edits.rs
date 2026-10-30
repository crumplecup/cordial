//! Build located edits from collected facts, without inspecting source syntax.

use std::ops::ControlFlow;

use super::return_lift::{ReturnLiftBlocker, ReturnLiftEdit};
use crate::ir::{
    DeclaredFunctionOutput, FunctionSourceSyntax, FunctionSuccessExit, SourceByteRange,
};

type EditDecision = ControlFlow<ReturnLiftBlocker, Vec<ReturnLiftEdit>>;

#[tracing::instrument(level = "trace", skip(facts))]
pub(super) fn edits_from_facts(facts: &FunctionSourceSyntax) -> EditDecision {
    let signature = facts.signature_range();
    let body = facts.body_range();
    if !contains(signature, facts.output_range())
        || signature.end() > body.start()
        || body.end().saturating_sub(body.start()) < 2
    {
        return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
    }
    let mut edits = vec![signature_edit(facts)];
    edits.extend(success_edits(facts)?);
    edits.sort_by_key(|edit| (edit.range().start(), edit.range().end()));
    ControlFlow::Continue(edits)
}

#[tracing::instrument(level = "trace", skip(facts))]
fn signature_edit(facts: &FunctionSourceSyntax) -> ReturnLiftEdit {
    let (prefix, original) = match facts.declared_output() {
        DeclaredFunctionOutput::ImplicitUnit => (" -> ", "()"),
        DeclaredFunctionOutput::Explicit(syntax) => ("", syntax.as_str()),
    };
    ReturnLiftEdit::new(
        facts.output_range().clone(),
        format!("{prefix}::core::result::Result<{original}, ::core::convert::Infallible>"),
    )
}

#[tracing::instrument(level = "trace", skip(facts))]
fn success_edits(facts: &FunctionSourceSyntax) -> EditDecision {
    let mut edits = Vec::new();
    let exits = facts.return_flow().success_exits();
    for (index, exit) in exits.iter().enumerate() {
        if exits[..index].contains(exit) {
            return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
        }
        let range = match exit {
            FunctionSuccessExit::Expression(range) => range.clone(),
            FunctionSuccessExit::BareReturn { offset }
            | FunctionSuccessExit::BlockUnit { offset }
            | FunctionSuccessExit::MissingElse { offset } => (*offset..*offset).into(),
        };
        if !inside_body(facts.body_range(), &range) {
            return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
        }
        match exit {
            FunctionSuccessExit::Expression(range) => {
                if range.start() == range.end() {
                    return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
                }
                wrap(&mut edits, range);
            }
            FunctionSuccessExit::BareReturn { offset } => {
                insert(&mut edits, *offset, " ::core::result::Result::Ok(())");
            }
            FunctionSuccessExit::BlockUnit { offset } => {
                insert(&mut edits, *offset, "\n::core::result::Result::Ok(())\n");
            }
            FunctionSuccessExit::MissingElse { offset } => {
                insert(
                    &mut edits,
                    *offset,
                    " else { ::core::result::Result::Ok(()) }",
                );
            }
        }
    }
    ControlFlow::Continue(edits)
}

#[tracing::instrument(level = "trace", skip(outer, inner))]
fn contains(outer: &SourceByteRange, inner: &SourceByteRange) -> bool {
    outer.start() <= inner.start() && inner.start() <= inner.end() && inner.end() <= outer.end()
}

#[tracing::instrument(level = "trace", skip(body, range))]
fn inside_body(body: &SourceByteRange, range: &SourceByteRange) -> bool {
    body.start() < range.start() && range.start() <= range.end() && range.end() < body.end()
}

#[tracing::instrument(level = "trace", skip(edits, range))]
fn wrap(edits: &mut Vec<ReturnLiftEdit>, range: &SourceByteRange) {
    insert(edits, range.start(), "::core::result::Result::Ok(");
    insert(edits, range.end(), ")");
}

#[tracing::instrument(level = "trace", skip(edits, text))]
fn insert(edits: &mut Vec<ReturnLiftEdit>, offset: usize, text: &str) {
    edits.push(ReturnLiftEdit::new(
        (offset..offset).into(),
        text.to_owned(),
    ));
}
