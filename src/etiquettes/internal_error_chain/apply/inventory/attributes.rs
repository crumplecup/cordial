//! Storage contracts of known derives, collected from the shared syntax tree.

use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Attribute, Expr, Meta, Token};
use tracing::instrument;

use super::super::getters::known_derive;
use super::helpers::derive_is_nonconstructing;

/// Display and Error derive no constructors. Display arguments may still
/// depend on String operations, so accept only direct uses of this field.
#[instrument(level = "debug", skip(attr, attrs, syntax))]
pub(in crate::etiquettes::internal_error_chain::apply) fn call_site_attribute_is_stable(
    attr: &Attribute,
    attrs: &[Attribute],
    field_name: &str,
    syntax: &syn::File,
) -> bool {
    stable_meta(&attr.meta, attrs, field_name, syntax)
}

/// Every possible conditional Display expansion must be storage independent.
/// Presence changes and conditional derives need their own contracts.
#[instrument(level = "trace", skip(meta, attrs, syntax))]
fn stable_meta(meta: &Meta, attrs: &[Attribute], field_name: &str, syntax: &syn::File) -> bool {
    if meta.path().is_ident("cfg_attr") {
        let Meta::List(list) = meta else { return false };
        let Ok(args) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
            return false;
        };
        return args.len() >= 2
            && args.iter().skip(1).all(|nested| {
                (nested.path().is_ident("display") || nested.path().is_ident("cfg_attr"))
                    && stable_meta(nested, attrs, field_name, syntax)
            });
    }
    if meta.path().is_ident("derive") {
        let Meta::List(list) = meta else { return false };
        return list
            .parse_nested_meta(|meta| {
                let path = &meta.path;
                let known = path.segments.len() == 1
                    && path.get_ident().is_some_and(|name| {
                        matches!(
                            name.to_string().as_str(),
                            "Debug"
                                | "Clone"
                                | "Copy"
                                | "PartialEq"
                                | "Eq"
                                | "Hash"
                                | "PartialOrd"
                                | "Ord"
                                | "Error"
                        )
                    })
                    || is_derive_more(path, "Display")
                    || is_derive_more(path, "Error")
                    || known_derive(syntax, path, "derive_getters", "Getters")
                    || known_derive(syntax, path, "derive_more", "Display");
                if known {
                    Ok(())
                } else {
                    Err(meta.error("unknown generated storage contract"))
                }
            })
            .is_ok();
    }
    if !meta.path().is_ident("display") || !has_display_derive(attrs, syntax) {
        return false;
    }
    let Meta::List(list) = meta else { return false };
    let Ok(args) = list.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated) else {
        return false;
    };
    if !matches!(args.first(), Some(Expr::Lit(lit)) if matches!(lit.lit, syn::Lit::Str(_))) {
        return false;
    }
    args.iter().skip(1).all(|expr| {
        matches!(expr, Expr::Path(path) if path.qself.is_none() && path.path.is_ident(field_name))
            || !expression_mentions_field(expr, field_name)
    })
}

#[instrument(level = "trace", skip(path))]
fn is_derive_more(path: &syn::Path, name: &str) -> bool {
    path.segments.len() == 2
        && path.segments[0].ident == "derive_more"
        && path.segments[1].ident == name
}

#[instrument(level = "trace", skip(attrs, syntax))]
fn has_display_derive(attrs: &[Attribute], syntax: &syn::File) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") || derive_is_nonconstructing(attr) {
            return false;
        }
        let mut found = false;
        attr.parse_nested_meta(|meta| {
            found |= is_derive_more(&meta.path, "Display")
                || known_derive(syntax, &meta.path, "derive_more", "Display");
            Ok(())
        })
        .is_ok()
            && found
    })
}

#[instrument(level = "trace", skip(expr))]
fn expression_mentions_field(expr: &Expr, field_name: &str) -> bool {
    let mut visitor = FieldMention {
        field_name,
        found: false,
    };
    visitor.visit_expr(expr);
    visitor.found
}

struct FieldMention<'a> {
    field_name: &'a str,
    found: bool,
}

impl<'ast> Visit<'ast> for FieldMention<'_> {
    #[instrument(level = "trace", skip(self, name))]
    fn visit_ident(&mut self, name: &'ast syn::Ident) {
        self.found |= name == self.field_name || name == "self" || name == "Self";
    }

    #[instrument(level = "trace", skip(self, mac))]
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // Expansion may introduce storage-dependent uses not visible here.
        self.found = true;
        syn::visit::visit_macro(self, mac);
    }
}
