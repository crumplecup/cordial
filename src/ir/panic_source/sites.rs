use super::{Context, Scanner, syntax};
use crate::CordialResult;
use crate::etiquettes::panics::{PanicKind, PanicSiteRecord};
use ra_ap_syntax::{AstNode, ast, ast::HasArgList};

impl Scanner<'_, '_> {
    #[tracing::instrument(level = "debug", skip(self, context, call), err(level = "warn"))]
    pub(super) fn method(
        &mut self,
        call: &ast::MethodCallExpr,
        context: &Context,
    ) -> CordialResult<()> {
        let Some(name) = call.name_ref() else {
            return Ok(());
        };
        let name = name.text();
        let line = self.source.line(call.syntax().text_range().start());
        if matches!(name, "expect_err" | "unwrap_err") && self.assertion_lines.contains(&line) {
            return Ok(());
        }
        let (kind, snippet) = match name {
            "expect" | "expect_err" => {
                let message = call
                    .arg_list()
                    .and_then(|list| list.args().next())
                    .and_then(|expr| {
                        let ast::Expr::Literal(lit) = expr else {
                            return None;
                        };
                        let ast::LiteralKind::String(token) = lit.kind() else {
                            return None;
                        };
                        token.value().ok().map(|value| value.into_owned())
                    });
                (
                    PanicKind::Expect,
                    match message {
                        Some(message) => format!(".{name}(\"{message}\")"),
                        None => format!(".{name}(…)"),
                    },
                )
            }
            "unwrap" | "unwrap_err" => (PanicKind::Unwrap, format!(".{name}()")),
            _ => return Ok(()),
        };
        self.push(kind, line, snippet, context)
    }

    #[tracing::instrument(level = "debug", skip(self, invocation, context), err(level = "warn"))]
    pub(super) fn invocation(
        &mut self,
        invocation: &ast::MacroCall,
        context: &Context,
    ) -> CordialResult<()> {
        let Some(path) = invocation.path() else {
            return Ok(());
        };
        if path.syntax().text() == "verus" {
            #[cfg(not(feature = "verus_ir"))]
            self.recovered_verus(invocation, context)?;
            return Ok(());
        }
        let Some(name) = syntax::path_name(&path) else {
            return Ok(());
        };
        let kind = match name.as_str() {
            "panic" => PanicKind::Panic,
            "unreachable" => PanicKind::Unreachable,
            "compile_error" => PanicKind::CompileError,
            _ => return Ok(()),
        };
        let args = invocation
            .token_tree()
            .map(|tree| crate::loader::database_token_text(tree.syntax()))
            .unwrap_or_default();
        let args = args
            .get(1..args.len().saturating_sub(1))
            .unwrap_or_default()
            .trim();
        let args = if args.chars().count() > 72 {
            format!("{}…", args.chars().take(72).collect::<String>())
        } else {
            args.into()
        };
        self.push(
            kind,
            self.source.line(path.syntax().text_range().start()),
            format!("{name}!({args})"),
            context,
        )
    }

    #[tracing::instrument(level = "debug", skip(self, context, kind), err(level = "warn"))]
    fn push(
        &mut self,
        kind: PanicKind,
        line: u32,
        snippet: String,
        context: &Context,
    ) -> CordialResult<()> {
        if !context.not_kani
            && context
                .functions
                .last()
                .is_some_and(|name| self.reachable.contains(name))
        {
            return Ok(());
        }
        let file = self
            .source
            .path()
            .strip_prefix(self.root)
            .unwrap_or(self.source.path())
            .to_path_buf();
        if kind == PanicKind::Unwrap
            && self.records.last().is_some_and(|last| {
                last.kind() == kind && last.line() == line && last.file() == &file
            })
        {
            return Ok(());
        }
        self.records.push(
            PanicSiteRecord::builder()
                .kind(kind)
                .context(context.label())
                .file(file)
                .line(line)
                .snippet(snippet)
                .cfg_test(context.cfg_test)
                .build()?,
        );
        Ok(())
    }
}
