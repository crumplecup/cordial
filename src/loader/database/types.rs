//! Convert typed HIR contracts to bounded shared-IR facts, never hover strings.

use ra_ap_hir::{Adt, Crate, ModuleDef, Semantics, Type};
use ra_ap_ide_db::{RootDatabase, famous_defs::FamousDefs};
use salsa::plumbing::AsId;
use tracing::instrument;

use crate::{SemanticItemId, SemanticReturnContract, SemanticType};

#[salsa::interned]
struct ItemKey<'db> {
    definition: ModuleDef,
}

#[instrument(level = "trace", skip(db, definition))]
pub(super) fn identity(db: &RootDatabase, definition: ModuleDef) -> SemanticItemId {
    SemanticItemId::new(ItemKey::new(db, definition).as_id().as_bits())
}

#[instrument(level = "trace", skip(db, ty))]
fn type_fact(db: &RootDatabase, ty: &Type<'_>) -> SemanticType {
    if ty.is_unknown() {
        SemanticType::Unknown
    } else if ty.is_unit() {
        SemanticType::Unit
    } else if ty.is_never() {
        SemanticType::Never
    } else if let Some((adt, args)) = ty.as_adt_with_args() {
        // Do not conflate different instantiations of a generic ADT. Projecting
        // the full recursive type is a later capability, not a name fallback.
        if !args.is_empty() {
            return SemanticType::Unsupported;
        }
        SemanticType::Adt {
            definition: identity(db, adt.into()),
            label: adt.name(db).as_str().to_owned(),
        }
    } else if let Some(scalar) = ty.as_builtin() {
        SemanticType::Scalar(scalar.name().as_str().to_owned())
    } else {
        SemanticType::Unsupported
    }
}

#[instrument(level = "trace", skip(sema, krate, ty))]
pub(super) fn return_contract(
    sema: &Semantics<'_, RootDatabase>,
    krate: Crate,
    ty: &Type<'_>,
) -> SemanticReturnContract {
    if let Some(Adt::Enum(adt)) = ty.as_adt() {
        let Some(result) = FamousDefs(sema, krate).core_result_Result() else {
            return SemanticReturnContract::Unavailable;
        };
        if adt == result {
            let mut args = ty.type_arguments();
            let (Some(success), Some(error)) = (args.next(), args.next()) else {
                return SemanticReturnContract::Unavailable;
            };
            return SemanticReturnContract::Result {
                definition: identity(sema.db, result.into()),
                success: type_fact(sema.db, &success),
                error: type_fact(sema.db, &error),
            };
        }
    }
    SemanticReturnContract::Other(type_fact(sema.db, ty))
}

#[instrument(level = "trace", skip(contract))]
pub(super) fn incomplete(contract: &SemanticReturnContract) -> bool {
    match contract {
        SemanticReturnContract::Result { success, error, .. } => [success, error]
            .into_iter()
            .any(|ty| matches!(ty, SemanticType::Unknown | SemanticType::Unsupported)),
        SemanticReturnContract::Other(ty) => {
            matches!(ty, SemanticType::Unknown | SemanticType::Unsupported)
        }
        SemanticReturnContract::Unavailable => true,
    }
}
