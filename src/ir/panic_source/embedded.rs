use super::{Context, Scanner, proof::ProofCollector};
use crate::CordialResult;
use crate::etiquettes::panics::PanicSiteRecord;
use ra_ap_syntax::{AstNode, ast};

impl Scanner<'_, '_> {
    #[cfg(not(feature = "verus_ir"))]
    #[tracing::instrument(level = "debug", skip(self, invocation, context), err(level = "warn"))]
    pub(super) fn recovered_verus(
        &mut self,
        invocation: &ast::MacroCall,
        context: &Context,
    ) -> CordialResult<()> {
        self.source.with_recovered_verus(invocation, |source| {
            let mut inner = Scanner::new(source, self.root, self.reachable, self.embedded);
            inner.assertion_lines.clone_from(&self.assertion_lines);
            inner.walk(source.tree().syntax(), context)?;
            self.assertion_lines.extend(inner.assertion_lines);
            self.records.extend(inner.records);
            Ok(())
        })
    }

    #[tracing::instrument(level = "debug", skip(self, literal, context), err(level = "warn"))]
    pub(super) fn embedded_program(
        &mut self,
        literal: &ast::Literal,
        context: &Context,
    ) -> CordialResult<()> {
        if self.embedded {
            return Ok(());
        }
        let origin = self.source.line(literal.syntax().text_range().start());
        let records = self
            .source
            .with_embedded_program(literal, |source| -> CordialResult<_> {
                let mut proofs = ProofCollector::default();
                proofs.walk(source.tree().syntax(), &source.panic_macro_syntax(), false);
                let reachable = proofs.finish().reachable();
                let mut inner = Scanner::new(source, self.root, &reachable, true);
                let mut context = context.clone();
                context.not_kani = false;
                inner.walk(source.tree().syntax(), &context)?;
                #[cfg(feature = "verus_ir")]
                {
                    let verus = source.verus_facts(&context.label());
                    let records = crate::etiquettes::panics::verus_findings(&verus, self.root)?;
                    for record in records {
                        inner.records.push(
                            PanicSiteRecord::builder()
                                .kind(record.kind())
                                .context(record.context().clone())
                                .file(record.file().clone())
                                .line(record.line())
                                .snippet(record.snippet().clone())
                                .cfg_test(context.cfg_test)
                                .build()?,
                        );
                    }
                }
                Ok(inner.records)
            })
            .transpose()?
            .unwrap_or_default();
        for record in records {
            self.records.push(
                PanicSiteRecord::builder()
                    .kind(record.kind())
                    .context(record.context().clone())
                    .file(record.file().clone())
                    .line(origin.saturating_add(record.line().saturating_sub(1)))
                    .snippet(record.snippet().clone())
                    .cfg_test(record.cfg_test())
                    .build()?,
            );
        }
        Ok(())
    }
}
