use std::collections::HashSet;
use std::path::Path;

use syn::Type;

use crate::error::CordialResult;
use crate::ir::SourceTracingInventory;

use super::recipe::recipe as instrument_recipe;
use super::types::FunctionRecord;

use tracing::instrument;

impl SourceTracingInventory {
    /// Apply run-specific configuration and proof reachability to collected facts.
    #[instrument(
        level = "debug",
        skip(self, never_instrument, file),
        err(level = "warn")
    )]
    pub(crate) fn records(
        &self,
        file: &Path,
        module_prefix: &str,
        project_root: &Path,
        extra_skip: &[String],
        never_instrument: &HashSet<String>,
    ) -> CordialResult<Vec<FunctionRecord>> {
        let relative = file
            .strip_prefix(project_root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let mut records = Vec::new();
        for function in self.functions() {
            let qualified = if module_prefix.is_empty() {
                function.path().clone()
            } else {
                format!("{module_prefix}::{}", function.path())
            };
            let proof_only = never_instrument.contains(&qualified);
            if proof_only && !function.instrumented() {
                continue;
            }
            let ctx = function.context();
            records.push(
                FunctionRecord::builder()
                    .qualified_name(qualified)
                    .kind(function.kind())
                    .visibility(function.visibility().clone())
                    .file(relative.clone())
                    .line(function.line())
                    .instrumented(function.instrumented())
                    .proof_only(proof_only)
                    .prover_visible_instrument(function.prover_visible_instrument())
                    .has_error_path_event(ctx.has_error_path_event())
                    .param_names(ctx.param_names().clone())
                    .role(ctx.role())
                    .complexity(ctx.complexity())
                    .recipe(instrument_recipe(ctx, extra_skip)?)
                    .build()?,
            );
        }
        Ok(records)
    }
}

#[instrument(level = "debug", skip(ty))]
pub(crate) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
pub(crate) fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// A `::`-free rendering of a type that *keeps* its generic arguments,
/// so `RustStdStandard<AtomicI8>` and `RustStdStandard<AtomicI16>`
/// produce distinct keys -- plain [`type_label`] collapses both to bare
/// `RustStdStandard`, which is exactly the collapse that let one
/// `impl Trait for RustStdStandard<T>` method stand in for every sibling
/// impl in the same module (checklist coverage hole). Lifetimes and path
/// prefixes are dropped; tuples, refs, slices, arrays and pointers are
/// rendered structurally; anything else falls back to [`type_label`].
#[instrument(level = "debug", skip(ty))]
pub(crate) fn self_type_key(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return "?".to_string();
            };
            let base = segment.ident.to_string();
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                return base;
            };
            let rendered: Vec<String> = args
                .args
                .iter()
                .filter_map(|arg| match arg {
                    syn::GenericArgument::Type(inner) => Some(self_type_key(inner)),
                    syn::GenericArgument::Const(expr) => Some(const_arg_label(expr)),
                    _ => None,
                })
                .collect();
            if rendered.is_empty() {
                base
            } else {
                format!("{base}<{}>", rendered.join(", "))
            }
        }
        Type::Reference(reference) => {
            let inner = self_type_key(&reference.elem);
            if reference.mutability.is_some() {
                format!("&mut {inner}")
            } else {
                format!("&{inner}")
            }
        }
        Type::Ptr(ptr) => {
            let inner = self_type_key(&ptr.elem);
            if matches!(ptr.mutability, syn::PointerMutability::Mut(_)) {
                format!("*mut {inner}")
            } else {
                format!("*const {inner}")
            }
        }
        Type::Tuple(tuple) => {
            let parts: Vec<String> = tuple.elems.iter().map(self_type_key).collect();
            format!("({})", parts.join(", "))
        }
        Type::Slice(slice) => format!("[{}]", self_type_key(&slice.elem)),
        Type::Array(array) => {
            format!(
                "[{}; {}]",
                self_type_key(&array.elem),
                const_arg_label(&array.len)
            )
        }
        Type::Paren(paren) => self_type_key(&paren.elem),
        Type::Group(group) => self_type_key(&group.elem),
        _ => type_label(ty),
    }
}

/// A short label for a const generic argument (`IntoIter<i32, 3>`'s `3`,
/// `[u8; 4]`'s `4`). Integer literals render as their digits; anything
/// more elaborate collapses to `_` -- distinctness only needs the common
/// literal case.
#[instrument(level = "trace", skip(expr))]
fn const_arg_label(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(int),
            ..
        }) => int.base10_digits().to_string(),
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Bool(b),
            ..
        }) => b.value.to_string(),
        syn::Expr::Path(path) => syn_path_label(&path.path),
        _ => "_".to_string(),
    }
}
