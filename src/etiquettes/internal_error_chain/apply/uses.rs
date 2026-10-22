//! Conservative use-site guard for type-changing error field edits.

use proc_macro2::{TokenStream, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, ExprField, File, Macro, Member, PatStruct, Token};
use tracing::instrument;

/// Only direct formatting reads are known to work with both `String` and `str`.
/// Unknown uses are blockers until the apply pass can rewrite or prove them.
#[instrument(level = "trace", skip(syntax))]
pub(super) fn has_unhandled_field_use(syntax: &File, type_name: &str, field_name: &str) -> bool {
    let mut visitor = FieldUseVisitor {
        type_name,
        field_name,
        unhandled: false,
    };
    visitor.visit_file(syntax);
    visitor.unhandled
}

#[instrument(level = "debug", skip(mac))]
pub(super) fn macro_has_unhandled_field_use(mac: &Macro, field_name: &str) -> bool {
    let mut visitor = FieldUseVisitor {
        type_name: "",
        field_name,
        unhandled: false,
    };
    visitor.visit_macro(mac);
    visitor.unhandled
}

struct FieldUseVisitor<'a> {
    type_name: &'a str,
    field_name: &'a str,
    unhandled: bool,
}

impl<'ast> Visit<'ast> for FieldUseVisitor<'_> {
    #[instrument(level = "debug", skip(self, expr))]
    fn visit_expr_field(&mut self, expr: &'ast ExprField) {
        if member_is_field(&expr.member, self.field_name) {
            // Without an inferred receiver type, this might refer to the
            // candidate error field. Decline instead of assuming otherwise.
            self.unhandled = true;
        }
        syn::visit::visit_expr_field(self, expr);
    }

    #[instrument(level = "debug", skip(self, pat))]
    fn visit_pat_struct(&mut self, pat: &'ast PatStruct) {
        if pat.path.is_ident(self.type_name)
            || pat.path.is_ident("Self")
            || pat
                .path
                .segments
                .last()
                .is_some_and(|part| part.ident == self.type_name)
        {
            if pat
                .fields
                .iter()
                .any(|field| member_is_field(&field.member, self.field_name))
            {
                self.unhandled = true;
            }
        }
        syn::visit::visit_pat_struct(self, pat);
    }

    #[instrument(level = "debug", skip(self, mac))]
    fn visit_macro(&mut self, mac: &'ast Macro) {
        if tokens_construct_type(&mac.tokens, self.type_name) {
            // The initializer visitor cannot see struct expressions inside
            // macro bodies or invocations.
            self.unhandled = true;
        }
        let uses_field = tokens_access_field(&mac.tokens, self.field_name);
        if !uses_field {
            return;
        }
        // A direct argument to the standard formatting macros is a Display
        // or Debug input in either strategy. Nested expressions, named
        // arguments, and unrecognized macros need more analysis.
        let is_write = mac.path.is_ident("write") || mac.path.is_ident("writeln");
        if !is_write {
            self.unhandled = true;
            return;
        }
        let Ok(args) = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(mac.tokens.clone())
        else {
            self.unhandled = true;
            return;
        };
        if args.len() < 3
            || !matches!(args.iter().nth(1), Some(Expr::Lit(lit)) if matches!(lit.lit, syn::Lit::Str(_)))
        {
            self.unhandled = true;
            return;
        }
        for (index, arg) in args.iter().enumerate() {
            if !expr_accesses_field(arg, self.field_name) {
                continue;
            }
            if index < 2 || !is_direct_self_field(arg, self.field_name) {
                self.unhandled = true;
            }
        }
    }
}

#[instrument(level = "debug", skip(member))]
fn member_is_field(member: &Member, field_name: &str) -> bool {
    matches!(member, Member::Named(name) if name == field_name)
}

#[instrument(level = "trace", skip(expr), ret)]
fn is_direct_self_field(expr: &Expr, field_name: &str) -> bool {
    let Expr::Field(field) = expr else {
        return false;
    };
    member_is_field(&field.member, field_name)
        && matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self"))
}

#[instrument(level = "debug", skip(expr))]
fn expr_accesses_field(expr: &Expr, field_name: &str) -> bool {
    let mut visitor = FieldUseVisitor {
        type_name: "",
        field_name,
        unhandled: false,
    };
    visitor.visit_expr(expr);
    visitor.unhandled
}

#[instrument(level = "debug", skip(tokens))]
pub(super) fn tokens_access_field(tokens: &TokenStream, field_name: &str) -> bool {
    let trees: Vec<_> = tokens.clone().into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        if let TokenTree::Group(group) = tree {
            if tokens_access_field(&group.stream(), field_name) {
                return true;
            }
        }
        if matches!(tree, TokenTree::Punct(dot) if dot.as_char() == '.')
            && matches!(trees.get(index + 1), Some(TokenTree::Ident(name)) if name == field_name)
            && !matches!(trees.get(index + 2), Some(TokenTree::Group(group)) if group.delimiter() == proc_macro2::Delimiter::Parenthesis)
        {
            return true;
        }
    }
    false
}

#[instrument(level = "debug", skip(tokens))]
pub(super) fn tokens_construct_type(tokens: &TokenStream, type_name: &str) -> bool {
    let trees: Vec<_> = tokens.clone().into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        if let TokenTree::Group(group) = tree {
            if tokens_construct_type(&group.stream(), type_name) {
                return true;
            }
        }
        if matches!(tree, TokenTree::Ident(name) if name == type_name || name == "Self")
            && matches!(trees.get(index + 1), Some(TokenTree::Group(group)) if group.delimiter() == proc_macro2::Delimiter::Brace)
        {
            return true;
        }
    }
    false
}
