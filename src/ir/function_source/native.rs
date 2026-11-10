//! Project canonical rust-analyzer syntax into the existing function contract.

use ra_ap_syntax::{AstNode, TextRange};

use super::{DeclaredFunctionOutput, FunctionReturnFlow, FunctionSourceSyntax, calls, output};
use crate::loader::DatabaseSource;

/// Signature bytes exclude attributes and visibility, matching edit operations.
#[tracing::instrument(level = "trace", skip(function))]
pub(crate) fn signature_range(function: &ra_ap_syntax::ast::Fn) -> Option<TextRange> {
    let start = [
        function.fn_token().map(|token| token.text_range().start()),
        function
            .const_token()
            .map(|token| token.text_range().start()),
        function
            .async_token()
            .map(|token| token.text_range().start()),
        function
            .unsafe_token()
            .map(|token| token.text_range().start()),
        function.abi().map(|abi| abi.syntax().text_range().start()),
    ]
    .into_iter()
    .flatten()
    .min()?;
    let end = function
        .syntax()
        .children()
        .filter(|node| {
            !matches!(
                node.kind(),
                ra_ap_syntax::SyntaxKind::ATTR
                    | ra_ap_syntax::SyntaxKind::VISIBILITY
                    | ra_ap_syntax::SyntaxKind::BLOCK_EXPR
            )
        })
        .last()?
        .text_range()
        .end();
    Some(TextRange::new(start, end))
}

impl FunctionSourceSyntax {
    /// Locate the function inside the database-issued carrier. A range from a
    /// different snapshot cannot supply an unrelated AST alongside this text.
    #[tracing::instrument(level = "trace", skip(file, range))]
    #[hotpath::measure(impl_type = "FunctionSourceSyntax")]
    pub(crate) fn collect(file: &DatabaseSource<'_>, range: TextRange) -> Option<Self> {
        let function = file.function(range)?;
        let body = function.body()?;
        let signature = signature_range(&function)?;
        let output = function.ret_type().and_then(|output| output.ty());
        let declared_output = output
            .as_ref()
            .map_or(DeclaredFunctionOutput::ImplicitUnit, |ty| {
                DeclaredFunctionOutput::Explicit(file.text(ty.syntax()).to_owned())
            });
        let output_range = output.as_ref().map_or_else(
            || TextRange::empty(signature.end()),
            |ty| ty.syntax().text_range(),
        );
        let (calls, unwraps, has_call_shadowing, opaque_calls) = calls::collect(file, &function);
        let result_contract = output::native_result_contract(file, output.as_ref());
        // Exact generated syntax, deliberately separate from resolved Result identity.
        let has_infallible_result_syntax = result_contract.as_ref().is_some_and(|contract| {
            contract.error_identity().as_deref() == Some("::core::convert::Infallible")
                && output::native_core_result_syntax(output.as_ref())
        });
        Some(Self {
            source_digest: file.source_digest(),
            signature: file.source()[std::ops::Range::<usize>::from(signature)].to_owned(),
            declared_output,
            signature_range: std::ops::Range::<usize>::from(signature).into(),
            body_range: std::ops::Range::<usize>::from(body.syntax().text_range()).into(),
            output_range: std::ops::Range::<usize>::from(output_range).into(),
            return_flow: FunctionReturnFlow::collect(&function),
            has_infallible_result_syntax,
            result_contract,
            calls,
            unwraps,
            has_call_shadowing,
            entry_contract: output::native_entry_contract(&function),
            opaque_calls: Some(opaque_calls),
        })
    }
}
