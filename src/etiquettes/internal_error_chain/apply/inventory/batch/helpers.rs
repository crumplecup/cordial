//! Site recording and return-scope helpers for the shared inventory walk.

use tracing::instrument;

use super::super::helpers::return_type_mentions;
use super::super::try_conversion::{ErrorTryEvidence, ErrorTryScope};
use super::super::{ErrorConversionSite, ErrorConversionSiteKind};
use super::BatchInventoryVisitor;

impl BatchInventoryVisitor<'_> {
    #[instrument(level = "debug", skip(self, kind, span))]
    pub(super) fn record(
        &mut self,
        index: usize,
        kind: ErrorConversionSiteKind,
        span: proc_macro2::Span,
        resolved: bool,
    ) {
        let start = span.start();
        self.sites[index].push(ErrorConversionSite {
            path: self.file.path().to_path_buf(),
            line: start.line,
            column: start.column,
            kind,
            edge: None,
            from_conversion: None,
            source_method: None,
            try_conversion: None,
            reference_path: None,
            target: None,
            field_accessible: true,
            resolved: resolved
                && self.file.path() == self.candidates[index].declaration
                && self.module_depth == 0,
        });
    }

    #[instrument(level = "debug", skip(self, kind, span, path))]
    pub(super) fn record_path(
        &mut self,
        index: usize,
        kind: ErrorConversionSiteKind,
        span: proc_macro2::Span,
        resolved: bool,
        path: &syn::Path,
        associated: bool,
    ) {
        self.record(index, kind, span, resolved);
        if self.module_depth != 0 {
            return;
        }
        let mut parts: Vec<String> = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        if associated {
            parts.pop();
        }
        if let Some(site) = self.sites[index].last_mut() {
            site.reference_path = Some(parts);
        }
    }

    #[instrument(level = "trace", skip(self, path))]
    pub(super) fn matches_type(&self, index: usize, path: &syn::Path) -> bool {
        path.segments
            .last()
            .is_some_and(|part| part.ident == self.candidates[index].type_name)
    }

    #[instrument(level = "trace", skip(self, path))]
    pub(super) fn direct_type(&self, index: usize, path: &syn::Path) -> bool {
        let name = &self.candidates[index].type_name;
        path.is_ident(name) || (path.is_ident("Self") && self.current_impl.as_ref() == Some(name))
    }

    #[instrument(level = "trace", skip(self, path))]
    pub(super) fn associated_path(&self, index: usize, path: &syn::Path) -> bool {
        let name = &self.candidates[index].type_name;
        path.segments.len() > 1
            && path.segments.first().is_some_and(|part| {
                part.ident == *name
                    || (part.ident == "Self" && self.current_impl.as_ref() == Some(name))
            })
    }

    #[instrument(level = "debug", skip(self, output, scope))]
    pub(super) fn enter_return_scope(
        &mut self,
        output: Option<&syn::ReturnType>,
        scope: ErrorTryScope,
    ) -> (Vec<bool>, Option<ErrorTryEvidence>) {
        let previous = (
            std::mem::replace(
                &mut self.returning_error,
                vec![false; self.candidates.len()],
            ),
            self.try_evidence.take(),
        );
        for &index in self.relevant {
            let name = &self.candidates[index].type_name;
            self.returning_error[index] = output.is_none_or(|output| {
                return_type_mentions(output, name)
                    || (self.current_impl.as_ref() == Some(name)
                        && return_type_mentions(output, "Self"))
            });
        }
        self.try_evidence = Some(ErrorTryEvidence::for_scope(output, scope));
        previous
    }

    #[instrument(level = "trace", skip(self, previous))]
    pub(super) fn leave_return_scope(&mut self, previous: (Vec<bool>, Option<ErrorTryEvidence>)) {
        self.returning_error = previous.0;
        self.try_evidence = previous.1;
    }
}
