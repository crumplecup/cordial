//! Build located edits from collected facts, without inspecting source syntax.

use std::ops::ControlFlow;

use super::return_lift::{ReturnLiftBlocker, ReturnLiftEdit};
use crate::ir::{
    DeclaredFunctionOutput, FunctionSourceSyntax, FunctionTail, ReturnValue, SourceByteRange,
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
    edits.extend(return_edits(facts)?);
    edits.extend(tail_edits(facts)?);
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
fn return_edits(facts: &FunctionSourceSyntax) -> EditDecision {
    let mut edits = Vec::new();
    let mut previous_end = facts.body_range().start();
    for site in facts.return_flow().returns() {
        if !inside_body(facts.body_range(), site.range()) || site.range().start() < previous_end {
            return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
        }
        previous_end = site.range().end();
        match site.value() {
            ReturnValue::Unit => insert(
                &mut edits,
                site.range().end(),
                " ::core::result::Result::Ok(())",
            ),
            ReturnValue::Expression(range) => {
                if !contains(site.range(), range) || range.start() == range.end() {
                    return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
                }
                wrap(&mut edits, range);
            }
        }
    }
    ControlFlow::Continue(edits)
}

#[tracing::instrument(level = "trace", skip(facts))]
fn tail_edits(facts: &FunctionSourceSyntax) -> EditDecision {
    let mut edits = Vec::new();
    match facts.return_flow().tail() {
        FunctionTail::Unit => insert(
            &mut edits,
            facts.body_range().end() - 1,
            "\n::core::result::Result::Ok(())\n",
        ),
        FunctionTail::Return => {}
        FunctionTail::Expression(range) => {
            if !inside_body(facts.body_range(), range) || range.start() == range.end() {
                return ControlFlow::Break(ReturnLiftBlocker::InvalidRanges);
            }
            if facts
                .return_flow()
                .returns()
                .iter()
                .any(|site| contains(range, site.range()))
            {
                return ControlFlow::Break(ReturnLiftBlocker::ReturnInsideTail);
            }
            wrap(&mut edits, range);
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
