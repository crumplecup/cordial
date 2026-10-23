use std::collections::{BTreeMap, BTreeSet};

use syn::{Expr, ExprCall, GenericArgument, PathArguments, Type};

use super::syntax::{bind_generics, has_bound_call, is_binding, scoped};
use super::{
    Conversion, Function, ManualSource, SignatureChainCheck, SignatureChainIndex,
    SignatureChainStatus, Ty,
};
use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

impl SignatureChainIndex {
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn assess(
        &self,
        crate_name: &str,
    ) -> CordialResult<(
        Vec<InternalErrorComplianceFinding>,
        Vec<SignatureChainCheck>,
    )> {
        let mut findings = Vec::new();
        let mut checks = Vec::new();
        for site in &self.conversions {
            if !has_bound_call(&site.body, &site.binding) {
                continue;
            }
            let hint = self.receiver_error(site);
            let flow = self.flow(&site.body, &site.binding, &site.scope, hint.as_ref());
            let status = match &flow {
                None => SignatureChainStatus::Unknown,
                Some(flow) if !self.is_known_error(&flow.input) => SignatureChainStatus::Unknown,
                Some(flow) => match self.reaches(&flow.output, &flow.input, &mut BTreeSet::new()) {
                    Reach::Yes => SignatureChainStatus::Preserved,
                    Reach::No => SignatureChainStatus::Broken,
                    Reach::Unknown => SignatureChainStatus::Unknown,
                },
            };
            checks.push(SignatureChainCheck {
                file: site.file.clone(),
                line: site.line,
                status,
                input_type: flow.as_ref().map(|flow| flow.input.path.clone()),
                output_type: flow.as_ref().map(|flow| flow.output.path.clone()),
            });
            if status != SignatureChainStatus::Broken {
                continue;
            }
            let Some(flow) = flow else {
                continue;
            };
            findings.push(
                InternalErrorComplianceFinding::builder()
                    .crate_name(crate_name.to_string())
                    .rule_id(InternalErrorComplianceId::DiscardTyped001)
                    .context(site.scope.join("::"))
                    .file(site.file.clone())
                    .line(site.line)
                    .snippet(format!(
                        "map_err conversion: {} cannot preserve {} as a source",
                        flow.output.path, flow.input.path
                    ))
                    .foreign_error_type(Some(flow.input.path))
                    .internal_constructor(Some(flow.output.path))
                    .build()?,
            );
        }
        Ok((findings, checks))
    }

    #[tracing::instrument(level = "debug", skip(self, site))]
    fn receiver_error(&self, site: &Conversion) -> Option<Ty> {
        let ty = match &site.receiver {
            Expr::Call(call) => {
                let Expr::Path(path) = call.func.as_ref() else {
                    return None;
                };
                let function = self.function(&path.path, &site.scope)?;
                self.resolve_type(
                    &function.output,
                    &function.scope,
                    &BTreeMap::new(),
                    function.self_ty.as_ref(),
                )?
            }
            Expr::Path(path) => {
                let name = path.path.get_ident()?.to_string();
                self.resolve_type(site.params.get(&name)?, &site.scope, &BTreeMap::new(), None)?
            }
            _ => return None,
        };
        (ty.path == "Result" || ty.path == "std::result::Result")
            .then(|| ty.args.get(1).cloned())
            .flatten()
    }

    #[tracing::instrument(level = "debug", skip(self, expr, hint))]
    fn flow(
        &self,
        expr: &Expr,
        binding: &str,
        scope: &[String],
        hint: Option<&Ty>,
    ) -> Option<Flow> {
        match expr {
            Expr::Block(block) => match block.block.stmts.last()? {
                syn::Stmt::Expr(tail, _) => self.flow(tail, binding, scope, hint),
                _ => None,
            },
            Expr::Paren(paren) => self.flow(&paren.expr, binding, scope, hint),
            Expr::Group(group) => self.flow(&group.expr, binding, scope, hint),
            Expr::Call(call) => self.call_flow(call, binding, scope, hint),
            _ => None,
        }
    }

    #[tracing::instrument(level = "debug", skip(self, call, hint))]
    fn call_flow(
        &self,
        call: &ExprCall,
        binding: &str,
        scope: &[String],
        hint: Option<&Ty>,
    ) -> Option<Flow> {
        let Expr::Path(path) = call.func.as_ref() else {
            return None;
        };
        let function = self.function(&path.path, scope)?;
        if call.args.len() != function.params.len() {
            return None;
        }
        let mut selected = None;
        for (arg, formal) in call.args.iter().zip(&function.params) {
            let inner = if is_binding(arg, binding) {
                Some((hint.cloned(), None))
            } else {
                self.flow(arg, binding, scope, hint)
                    .map(|flow| (Some(flow.output.clone()), Some(flow)))
            };
            if let Some((actual, prior)) = inner {
                if selected.is_some() {
                    return None;
                }
                selected = Some((formal, actual, prior));
            }
        }
        let (formal, actual, prior) = selected?;
        let mut bindings = BTreeMap::new();
        if let Some(actual) = &actual {
            bind_generics(formal, actual, &function.generics, &mut bindings)?;
        }
        let formal_ty = self.resolve_type(
            formal,
            &function.scope,
            &bindings,
            function.self_ty.as_ref(),
        )?;
        let input = prior.as_ref().map_or_else(
            || actual.clone().unwrap_or_else(|| formal_ty.clone()),
            |flow| flow.input.clone(),
        );
        // The receiver's known error type must agree with the helper parameter.
        if prior.is_none() && hint.is_some_and(|known| known != &formal_ty) {
            return None;
        }
        if prior.is_some() && actual.as_ref().is_some_and(|known| known != &formal_ty) {
            return None;
        }
        let output = self.resolve_type(
            &function.output,
            &function.scope,
            &bindings,
            function.self_ty.as_ref(),
        )?;
        Some(Flow { input, output })
    }

    #[tracing::instrument(level = "debug", skip(self, path))]
    fn function(&self, path: &syn::Path, scope: &[String]) -> Option<&Function> {
        let key = self.resolve_path(path, scope, None)?;
        let functions = self.functions.get(&key)?;
        (functions.len() == 1).then(|| &functions[0])
    }

    #[tracing::instrument(level = "debug", skip(self, ty, bindings, self_ty))]
    pub(super) fn resolve_type(
        &self,
        ty: &Type,
        scope: &[String],
        bindings: &BTreeMap<String, Ty>,
        self_ty: Option<&Ty>,
    ) -> Option<Ty> {
        match ty {
            Type::Reference(reference) => {
                self.resolve_type(&reference.elem, scope, bindings, self_ty)
            }
            Type::Paren(paren) => self.resolve_type(&paren.elem, scope, bindings, self_ty),
            Type::Group(group) => self.resolve_type(&group.elem, scope, bindings, self_ty),
            Type::Tuple(tuple) => Some(Ty {
                path: "tuple".to_string(),
                args: tuple
                    .elems
                    .iter()
                    .map(|elem| self.resolve_type(elem, scope, bindings, self_ty))
                    .collect::<Option<Vec<_>>>()?,
            }),
            Type::Path(path) if path.qself.is_none() => {
                let first = path.path.segments.first()?.ident.to_string();
                if path.path.segments.len() == 1 {
                    if let Some(bound) = bindings.get(&first) {
                        return Some(bound.clone());
                    }
                    if first == "Self" {
                        return self_ty.cloned();
                    }
                }
                let resolved = self.resolve_path(&path.path, scope, self_ty)?;
                let args = match &path.path.segments.last()?.arguments {
                    PathArguments::AngleBracketed(list) => list
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(ty) => {
                                Some(self.resolve_type(ty, scope, bindings, self_ty))
                            }
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                    PathArguments::None => Vec::new(),
                    _ => return None,
                };
                Some(Ty {
                    path: resolved,
                    args,
                })
            }
            _ => None,
        }
    }

    #[tracing::instrument(level = "trace", skip(self, path, self_ty))]
    pub(super) fn resolve_path(
        &self,
        path: &syn::Path,
        scope: &[String],
        self_ty: Option<&Ty>,
    ) -> Option<String> {
        let parts: Vec<String> = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        let first = parts.first()?;
        if first == "Self" {
            let ty = self_ty?;
            return Some(
                std::iter::once(ty.path.as_str())
                    .chain(parts.iter().skip(1).map(String::as_str))
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        if first == "crate" {
            return Some(parts[1..].join("::"));
        }
        if first == "self" {
            return Some(scoped(scope, &parts[1..].join("::")));
        }
        if first == "super" {
            if scope.is_empty() {
                return None;
            }
            return Some(scoped(&scope[..scope.len() - 1], &parts[1..].join("::")));
        }
        if let Some(imported) = self
            .imports
            .get(scope)
            .and_then(|imports| imports.get(first))
        {
            let base = if let Some(rest) = imported.strip_prefix("crate::") {
                rest.to_string()
            } else if let Some(rest) = imported.strip_prefix("super::") {
                if scope.is_empty() {
                    return None;
                }
                scoped(&scope[..scope.len() - 1], rest)
            } else if let Some(rest) = imported.strip_prefix("self::") {
                scoped(scope, rest)
            } else {
                imported.clone()
            };
            return Some(
                std::iter::once(base.as_str())
                    .chain(parts.iter().skip(1).map(String::as_str))
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        let local = scoped(scope, &parts.join("::"));
        if self.functions.contains_key(&local) || self.errors.contains_key(&local) {
            return Some(local);
        }
        if matches!(
            first.as_str(),
            "String" | "str" | "Result" | "Option" | "Box"
        ) || matches!(first.as_str(), "std" | "core" | "alloc")
        {
            return Some(parts.join("::"));
        }
        None
    }

    #[tracing::instrument(level = "debug", skip(self, output, input, visited))]
    fn reaches(&self, output: &Ty, input: &Ty, visited: &mut BTreeSet<Ty>) -> Reach {
        if output == input {
            return Reach::Yes;
        }
        if output.path == "String" || output.path == "str" {
            return Reach::No;
        }
        if !visited.insert(output.clone()) {
            return Reach::No;
        }
        let Some(defs) = self.errors.get(&output.path) else {
            return Reach::Unknown;
        };
        if defs.len() != 1 {
            return Reach::Unknown;
        }
        let def = &defs[0];
        let impls = self.manual_impls.get(&output.path);
        if impls.is_some_and(|impls| impls.len() != 1) || def.params.len() != output.args.len() {
            return Reach::Unknown;
        }
        let manual = impls.and_then(|impls| impls.first());
        if !def.error_impl && manual.is_none() {
            return Reach::Unknown;
        }
        if matches!(manual, Some(ManualSource::Opaque)) {
            return Reach::Unknown;
        }
        if matches!(manual, Some(ManualSource::Default)) {
            return Reach::No;
        }
        let bindings: BTreeMap<_, _> = def
            .params
            .iter()
            .cloned()
            .zip(output.args.iter().cloned())
            .collect();
        let mut unknown = false;
        let sources: Vec<&Type> = match manual {
            Some(ManualSource::Variants(variants)) => {
                if variants
                    .iter()
                    .any(|variant| !def.variant_sources.contains_key(variant))
                {
                    return Reach::Unknown;
                }
                variants
                    .iter()
                    .filter_map(|variant| def.variant_sources.get(variant))
                    .collect()
            }
            _ => def.sources.iter().collect(),
        };
        for source in sources {
            let Some(target) = self.resolve_type(source, &def.scope, &bindings, Some(output))
            else {
                unknown = true;
                continue;
            };
            match self.reaches(&target, input, visited) {
                Reach::Yes => return Reach::Yes,
                Reach::Unknown => unknown = true,
                Reach::No => {}
            }
        }
        if unknown { Reach::Unknown } else { Reach::No }
    }

    #[tracing::instrument(level = "trace", skip(self, ty))]
    fn is_known_error(&self, ty: &Ty) -> bool {
        self.errors
            .get(&ty.path)
            .is_some_and(|defs| defs.len() == 1 && defs[0].error_impl)
            || self
                .manual_impls
                .get(&ty.path)
                .is_some_and(|impls| impls.len() == 1)
            || matches!(
                ty.path.as_str(),
                "std::io::Error" | "std::fmt::Error" | "std::num::ParseIntError" | "syn::Error"
            )
    }
}

#[derive(Clone)]
struct Flow {
    input: Ty,
    output: Ty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    Yes,
    No,
    Unknown,
}
