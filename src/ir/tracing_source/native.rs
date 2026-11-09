//! Native tracing inventory: consume the canonical tree and existing semantic facts.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasName, HasVisibility},
};
use tracing::instrument;

use super::display::DisplayFacts;
use super::{SourceTracingFunction, SourceTracingInventory, recordable};
use crate::CordialResult;
use crate::etiquettes::tracing::{
    BodyPeek, FnContext, FunctionKind, ReceiverShape, VisibilityLabel, classify_complexity,
    classify_role,
};
use crate::ir::{
    SemanticCapability, SemanticFunction, SemanticResultRecording, SourceFileAttributes,
};
use crate::loader::DatabaseSource;

impl SourceTracingInventory {
    /// All syntax comes from the database-issued carrier. Semantic evidence is
    /// joined by declaration range, including same-named cfg alternatives.
    #[instrument(
        level = "debug",
        skip(file, attributes, semantics),
        err(level = "warn")
    )]
    #[hotpath::measure(impl_type = "SourceTracingInventory")]
    pub(crate) fn collect_native(
        file: &DatabaseSource<'_>,
        attributes: &SourceFileAttributes,
        semantics: &HashMap<TextRange, SemanticFunction>,
    ) -> CordialResult<Self> {
        let mut functions = Vec::new();
        let display = DisplayFacts::collect(file);
        for item in attributes.items() {
            let range = TextRange::new(
                (item.range().start() as u32).into(),
                (item.range().end() as u32).into(),
            );
            let Some(function) = file.function(range) else {
                continue;
            };
            if function.const_token().is_some() {
                continue;
            }
            let implementation = function.syntax().ancestors().find_map(ast::Impl::cast);
            let kind = match implementation {
                Some(implementation) if implementation.trait_().is_some() => {
                    FunctionKind::TraitImplMethod
                }
                Some(_) => FunctionKind::InherentMethod,
                None => FunctionKind::Free,
            };
            let context = context(file, &function, kind, semantics.get(&range), &display)?;
            functions.push(SourceTracingFunction {
                path: item.path().clone(),
                range: item.range().clone(),
                line: item.line(),
                kind,
                visibility: visibility(&function),
                instrumented: item.attributes().iter().any(|attr| attr.instrument()),
                prover_visible_instrument: item
                    .attributes()
                    .iter()
                    .any(|attr| attr.instrument() && attr.path() != "cfg_attr"),
                context,
            });
        }
        Ok(Self {
            source_digest: file.source_digest(),
            functions,
        })
    }
}

#[instrument(
    level = "debug",
    skip(file, function, semantics, kind, display),
    err(level = "warn")
)]
fn context(
    file: &DatabaseSource<'_>,
    function: &ast::Fn,
    kind: FunctionKind,
    semantics: Option<&SemanticFunction>,
    display: &DisplayFacts,
) -> CordialResult<FnContext> {
    let output = function.ret_type().and_then(|ret| ret.ty());
    let syntactic_result = output.as_ref().is_some_and(recordable::returns_result);
    let (returns_result, displayable) = match semantics.map(|facts| facts.result_recording()) {
        Some(SemanticResultRecording::Result {
            display: capability,
            ..
        }) => (
            true,
            *capability == SemanticCapability::Proven
                || output
                    .as_ref()
                    .is_some_and(|ty| display.err_is_displayable(ty)),
        ),
        Some(SemanticResultRecording::NotResult) => (false, false),
        // Inactive cfg branches have no semantic facts; read their text.
        _ => (
            syntactic_result,
            syntactic_result
                && output
                    .as_ref()
                    .is_some_and(|ty| display.err_is_displayable(ty)),
        ),
    };
    let (peek, lines) = body(file, function);
    let receiver = function.param_list().and_then(|list| list.self_param());
    let shape = ReceiverShape::new(
        receiver.as_ref().is_some_and(|receiver| {
            receiver.amp_token().is_some() && receiver.mut_token().is_none()
        }),
        receiver.as_ref().is_some_and(|receiver| {
            receiver.amp_token().is_none()
                && receiver.colon_token().is_none()
                && receiver.mut_token().is_some()
        }),
        function
            .param_list()
            .map(|list| list.params().count() + usize::from(list.self_param().is_some()))
            .unwrap_or(0),
    );
    let name = function
        .name()
        .ok_or_else(|| crate::CordialError::workspace_source_facts_missing("function name"))?;
    let role = classify_role(
        name.text(),
        &shape,
        kind,
        &peek,
        output.as_ref().is_some_and(recordable::returns_self),
        output.as_ref().is_some_and(recordable::contains_bool),
        returns_result,
    );
    let (names, skipped) = recordable::parameters(function);
    FnContext::builder()
        .role(role)
        .complexity(classify_complexity(lines, returns_result, &peek))
        .param_names(names)
        .unrecordable_params(skipped)
        .returns_result(returns_result)
        .return_unrecordable(output.as_ref().is_some_and(recordable::unrecordable))
        .return_borrowed(output.as_ref().is_some_and(recordable::borrowed))
        .err_is_displayable(displayable)
        .has_error_path_event(peek.has_error_path_event())
        .build()
}

#[instrument(level = "debug", skip(file, function))]
fn body(file: &DatabaseSource<'_>, function: &ast::Fn) -> (BodyPeek, u32) {
    let Some(body) = function.body() else {
        return (BodyPeek::default(), 1);
    };
    let stmts = body.stmt_list().map_or(0, |list| {
        list.statements()
            .filter(|stmt| !matches!(stmt, ast::Stmt::Item(_)))
            .count()
            + usize::from(list.tail_expr().is_some())
    });
    let mut has_branch = false;
    let mut has_error_path_event = false;
    for node in body.syntax().descendants() {
        has_branch |= ast::IfExpr::can_cast(node.kind())
            || ast::MatchExpr::can_cast(node.kind())
            || ast::WhileExpr::can_cast(node.kind())
            || ast::ForExpr::can_cast(node.kind());
        if let Some(call) = ast::MacroCall::cast(node) {
            has_error_path_event |= call
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|name| matches!(name.text(), "warn" | "error"));
        }
    }
    let range = body.syntax().text_range();
    (
        BodyPeek::new(stmts, has_branch, has_error_path_event),
        file.line(range.end())
            .saturating_sub(file.line(range.start()))
            .max(1),
    )
}

#[instrument(level = "debug", skip(function))]
fn visibility(function: &ast::Fn) -> VisibilityLabel {
    use ast::VisibilityKind;
    match function.visibility().map(|visibility| visibility.kind()) {
        Some(VisibilityKind::Pub) => VisibilityLabel::Public,
        Some(VisibilityKind::PubCrate) => VisibilityLabel::PubCrate,
        Some(VisibilityKind::PubSuper) => VisibilityLabel::PubSuper,
        Some(VisibilityKind::PubSelf) => VisibilityLabel::PubInPath("self".into()),
        Some(VisibilityKind::In(path)) => VisibilityLabel::PubInPath(
            path.syntax()
                .descendants()
                .filter_map(ast::PathSegment::cast)
                .filter_map(|segment| segment.name_ref())
                .next()
                .map(|name| name.text().to_string())
                .unwrap_or_default(),
        ),
        None => VisibilityLabel::Private,
    }
}
