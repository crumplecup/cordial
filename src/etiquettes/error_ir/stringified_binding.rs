//! Track a caught error binding through direct stringification and wrapping.

use syn::visit::Visit;
use syn::{Expr, ExprCall, Pat, Stmt};
use tracing::instrument;

/// Inspect `map_err` even when syntax does not reveal the receiver's error type.
#[instrument(level = "debug", skip(expr))]
pub(super) fn map_err_discards_stringified_binding(expr: &Expr) -> bool {
    let Expr::Closure(closure) = expr else {
        return false;
    };
    let Some(Pat::Ident(binding)) = closure.inputs.first() else {
        return false;
    };
    let name = binding.ident.to_string();
    discards_stringified_binding(&closure.body, &name)
}

#[instrument(level = "debug", skip(expr))]
pub(super) fn discards_stringified_binding(expr: &Expr, name: &str) -> bool {
    let mut all_uses = ErrorBindingUses::new(name);
    all_uses.visit_expr(expr);
    if !all_uses.stringified {
        return false;
    }
    let Some(result) = result_expr(expr) else {
        return true;
    };
    let mut result_uses = ErrorBindingUses::new(name);
    result_uses.visit_expr(result);
    !result_uses.retained
}

/// A side-effecting call earlier in a block does not preserve the source in
/// the error returned by its final expression.
#[instrument(level = "trace", skip(expr))]
fn result_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Block(block) => match block.block.stmts.last()? {
            Stmt::Expr(expr, _) => result_expr(expr),
            _ => None,
        },
        Expr::Paren(paren) => result_expr(&paren.expr),
        Expr::Group(group) => result_expr(&group.expr),
        Expr::Return(ret) => ret.expr.as_deref().and_then(result_expr),
        _ => Some(expr),
    }
}

struct ErrorBindingUses<'a> {
    name: &'a str,
    stringified: bool,
    retained: bool,
}

impl<'a> ErrorBindingUses<'a> {
    #[instrument(level = "debug", fields(name = name))]
    fn new(name: &'a str) -> Self {
        Self {
            name,
            stringified: false,
            retained: false,
        }
    }

    #[instrument(level = "trace", skip(self, expr))]
    fn is_binding(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Path(path) if path.path.get_ident().is_some_and(|ident| ident == self.name))
    }
}

impl<'ast> Visit<'ast> for ErrorBindingUses<'_> {
    #[instrument(level = "debug", skip(self, call))]
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "to_string" && self.is_binding(&call.receiver) {
            self.stringified = true;
            return;
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    #[instrument(level = "debug", skip(self, expr))]
    fn visit_expr_macro(&mut self, expr: &'ast syn::ExprMacro) {
        let name = expr.mac.path.segments.last().map(|segment| &segment.ident);
        if name.is_some_and(|name| {
            matches!(
                name.to_string().as_str(),
                "format" | "format_args" | "write" | "writeln"
            )
        }) && macro_interpolates_binding(&expr.mac.tokens.to_string(), self.name)
        {
            self.stringified = true;
        }
    }

    #[instrument(level = "debug", skip(self, call))]
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if call.args.iter().any(|arg| self.is_binding(arg)) {
            self.retained = true;
        }
        syn::visit::visit_expr_call(self, call);
    }

    #[instrument(level = "debug", skip(self, expr))]
    fn visit_expr_struct(&mut self, expr: &'ast syn::ExprStruct) {
        if expr.fields.iter().any(|field| {
            matches!(&field.member, syn::Member::Named(name) if name == "source")
                && self.is_binding(&field.expr)
        }) {
            self.retained = true;
        }
        syn::visit::visit_expr_struct(self, expr);
    }
}

#[instrument(level = "debug")]
fn macro_interpolates_binding(tokens: &str, binding: &str) -> bool {
    let compact = tokens.replace(' ', "");
    compact.contains(&format!("{{{binding}}}"))
        || compact.contains(&format!("{{{binding}:"))
        || compact.contains(&format!(",{binding}"))
        || compact.contains(&format!("({binding})"))
}
