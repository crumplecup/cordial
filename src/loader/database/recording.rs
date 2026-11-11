//! Project formatting obligations from resolved HIR into the shared graph.

use ra_ap_hir::{Adt, AsAssocItem, AssocItemContainer, Function, LangItem, Semantics, Trait, Type};
use ra_ap_ide_db::{RootDatabase, famous_defs::FamousDefs};

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

use crate::{
    SemanticCapability, SemanticFormattingRole, SemanticParameter, SemanticResultRecording,
    SemanticReturnRecording, SemanticReturnShape,
};

/// Check the instantiated error in the function's own parameter environment.
/// A borrowed success value has no bearing on the error's formatting traits.
#[tracing::instrument(level = "debug", skip(sema, function))]
#[hotpath::measure]
pub(super) fn result_recording(
    sema: &Semantics<'_, RootDatabase>,
    function: Function,
) -> SemanticResultRecording {
    let ty = function.ret_type(sema.db);
    if ty.is_unknown() {
        return SemanticResultRecording::Unavailable;
    }
    let definitions = FamousDefs(sema, function.module(sema.db).krate(sema.db));
    let Some(result) = definitions.core_result_Result() else {
        return SemanticResultRecording::Unavailable;
    };
    if ty.as_adt() != Some(Adt::Enum(result)) {
        return SemanticResultRecording::NotResult;
    }
    let Some(error) = ty.type_arguments().nth(1) else {
        return SemanticResultRecording::Unavailable;
    };
    SemanticResultRecording::Result {
        display: capability(sema.db, &error, definitions.core_fmt_Display()),
        debug: capability(
            sema.db,
            &error,
            Trait::lang(
                sema.db,
                function.module(sema.db).krate(sema.db),
                LangItem::Debug,
            ),
        ),
    }
}

/// Whether the whole return value can be recorded, and whether any part of its
/// type is a reference or raw pointer. Both read the resolved type, so an alias
/// that hides a reference, `Self`, and `impl Trait` are judged as what they are.
#[tracing::instrument(level = "debug", skip(sema, function))]
pub(super) fn return_recording(
    sema: &Semantics<'_, RootDatabase>,
    function: Function,
) -> SemanticReturnRecording {
    let ty = function.ret_type(sema.db);
    if ty.is_unknown() {
        return SemanticReturnRecording::default();
    }
    let debug = Trait::lang(
        sema.db,
        function.module(sema.db).krate(sema.db),
        LangItem::Debug,
    );
    let mut borrowed = false;
    ty.walk(sema.db, |part| {
        borrowed |= part.is_reference() || part.is_raw_ptr();
    });
    let verdict = capability(sema.db, &ty, debug);
    tracing::debug!(
        target: "cordial::semantic::recording",
        function = %function.name(sema.db).as_str(),
        ?verdict,
        borrowed,
        "return recording"
    );
    SemanticReturnRecording::new(verdict, Some(borrowed))
}

/// Whether the return type is the impl's own `Self` (through references and
/// aliases) and whether a `bool` occurs anywhere in it.
#[tracing::instrument(level = "debug", skip(sema, function))]
pub(super) fn return_shape(
    sema: &Semantics<'_, RootDatabase>,
    function: Function,
) -> SemanticReturnShape {
    let ty = function.ret_type(sema.db);
    if ty.is_unknown() {
        return SemanticReturnShape::default();
    }
    let self_ty = function
        .as_assoc_item(sema.db)
        .and_then(|item| match item.container(sema.db) {
            AssocItemContainer::Impl(implementation) => Some(implementation.self_ty(sema.db)),
            AssocItemContainer::Trait(_) => None,
        });
    let is_self = self_ty.is_some_and(|self_ty| ty.strip_references() == self_ty);
    let mut has_bool = false;
    ty.walk(sema.db, |part| has_bool |= part.is_bool());
    tracing::debug!(
        target: "cordial::semantic::recording",
        function = %function.name(sema.db).as_str(),
        is_self,
        has_bool,
        "return shape"
    );
    SemanticReturnShape::new(Some(is_self), Some(has_bool))
}

/// The `Debug` verdict for every binding the function's parameters introduce,
/// judged on the binding's own type in the function's environment: a derived
/// `Debug`, a `Vec` of one, and a `T: Debug` parameter are all recordable.
#[tracing::instrument(level = "debug", skip(sema, function, source))]
pub(super) fn parameters(
    sema: &Semantics<'_, RootDatabase>,
    function: Function,
    source: &ast::Fn,
) -> Vec<SemanticParameter> {
    let debug = Trait::lang(
        sema.db,
        function.module(sema.db).krate(sema.db),
        LangItem::Debug,
    );
    let mut bindings = Vec::new();
    for pattern in source
        .param_list()
        .into_iter()
        .flat_map(|list| list.params())
        .filter_map(|param| param.pat())
    {
        for binding in pattern
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
        {
            let Some(name) = binding.name() else {
                continue;
            };
            let verdict = sema
                .type_of_pat(&ast::Pat::IdentPat(binding))
                .map_or(SemanticCapability::Unavailable, |info| {
                    capability(sema.db, &info.original, debug)
                });
            tracing::debug!(
                target: "cordial::semantic::recording",
                function = %function.name(sema.db).as_str(),
                parameter = %name.text(),
                ?verdict,
                "parameter recording"
            );
            bindings.push(SemanticParameter::new(name.text().to_string(), verdict));
        }
    }
    bindings
}

#[tracing::instrument(level = "trace", skip(db, ty, required))]
fn capability<'db>(
    db: &'db RootDatabase,
    ty: &Type<'db>,
    required: Option<Trait>,
) -> SemanticCapability {
    let Some(required) = required else {
        return SemanticCapability::Unavailable;
    };
    if ty.contains_unknown() {
        SemanticCapability::Unavailable
    } else if ty.impls_trait(db, required, &[]) {
        SemanticCapability::Proven
    } else {
        SemanticCapability::NotProven
    }
}

/// Trait identity distinguishes real formatters from local namesakes.
#[tracing::instrument(level = "trace", skip(sema, function))]
pub(super) fn formatting_role(
    sema: &Semantics<'_, RootDatabase>,
    function: Function,
) -> SemanticFormattingRole {
    let Some(item) = function.as_assoc_item(sema.db) else {
        return SemanticFormattingRole::Ordinary;
    };
    let implemented = match item.container(sema.db) {
        AssocItemContainer::Trait(trait_) => trait_,
        AssocItemContainer::Impl(impl_) => match impl_.trait_(sema.db) {
            Some(trait_) => trait_,
            None => {
                return if sema
                    .source(impl_)
                    .is_some_and(|source| source.value.trait_().is_none())
                {
                    SemanticFormattingRole::Ordinary
                } else {
                    SemanticFormattingRole::Unavailable
                };
            }
        },
    };
    let definitions = FamousDefs(sema, function.module(sema.db).krate(sema.db));
    let display = definitions.core_fmt_Display();
    let debug = Trait::lang(
        sema.db,
        function.module(sema.db).krate(sema.db),
        LangItem::Debug,
    );
    if Some(implemented) == display {
        SemanticFormattingRole::Display
    } else if Some(implemented) == debug {
        SemanticFormattingRole::Debug
    } else if display.is_some() && debug.is_some() {
        SemanticFormattingRole::Ordinary
    } else {
        SemanticFormattingRole::Unavailable
    }
}
