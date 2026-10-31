//! Project formatting obligations from resolved HIR into the shared graph.

use ra_ap_hir::{Adt, AsAssocItem, AssocItemContainer, Function, LangItem, Semantics, Trait, Type};
use ra_ap_ide_db::{RootDatabase, famous_defs::FamousDefs};

use crate::{SemanticCapability, SemanticFormattingRole, SemanticResultRecording};

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
