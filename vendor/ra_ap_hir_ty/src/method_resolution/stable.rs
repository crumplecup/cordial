//! What the impl indexes of a crate say, hashed so that it means the same in every process.
//! See `hir_expand::stable` for why, and for the two rules.
//!
//! The indexes (`InherentImpls`, `TraitImpls`) are made from every impl in a crate, so what read
//! one depends on every file with an impl in it. Their value is the same after an edit to a body.
//! This hashes exactly what is in them, with each id replaced by what it stands for, and takes
//! them apart without `..` so that a field rust-analyzer adds is a compile error here until
//! someone says what it adds.

use hir_def::AdtId;
use hir_expand::stable::{StableCx, StableFingerprint, StableHash};

use super::{InherentImpls, OneTraitImpls, TraitImpls};
use crate::{
    db::HirDatabase,
    next_solver::{Ctor, SimplifiedType, SolverDefId},
};
use base_db::Crate;

/// A simplified type, with the definition ids in it standing for what they are.
fn simplified(ty: &SimplifiedType<'_>, cx: &mut StableCx<'_>) {
    match ty {
        SimplifiedType::Bool => cx.tag("bool"),
        SimplifiedType::Char => cx.tag("char"),
        SimplifiedType::Int(int) => {
            cx.tag("int");
            cx.write_plain(int);
        }
        SimplifiedType::Uint(uint) => {
            cx.tag("uint");
            cx.write_plain(uint);
        }
        SimplifiedType::Float(float) => {
            cx.tag("float");
            cx.write_plain(float);
        }
        SimplifiedType::Adt(id) => {
            cx.tag("adt");
            definition(id, cx);
        }
        SimplifiedType::Foreign(id) => {
            cx.tag("foreign");
            definition(id, cx);
        }
        SimplifiedType::Str => cx.tag("str"),
        SimplifiedType::Array => cx.tag("array"),
        SimplifiedType::Slice => cx.tag("slice"),
        SimplifiedType::Ref(mutability) => {
            cx.tag("ref");
            cx.write_plain(mutability);
        }
        SimplifiedType::Ptr(mutability) => {
            cx.tag("ptr");
            cx.write_plain(mutability);
        }
        SimplifiedType::Never => cx.tag("never"),
        SimplifiedType::Tuple(arity) => {
            cx.tag("tuple");
            cx.write_u64(*arity as u64);
        }
        SimplifiedType::MarkerTraitObject => cx.tag("marker trait object"),
        SimplifiedType::Trait(id) => {
            cx.tag("trait object");
            definition(id, cx);
        }
        SimplifiedType::Closure(id) => {
            cx.tag("closure");
            definition(id, cx);
        }
        SimplifiedType::Coroutine(id) => {
            cx.tag("coroutine");
            definition(id, cx);
        }
        SimplifiedType::CoroutineWitness(id) => {
            cx.tag("coroutine witness");
            definition(id, cx);
        }
        SimplifiedType::Function(arity) => {
            cx.tag("function");
            cx.write_u64(*arity as u64);
        }
        SimplifiedType::UnsafeBinder => cx.tag("unsafe binder"),
        SimplifiedType::Placeholder => cx.tag("placeholder"),
        SimplifiedType::Error => cx.tag("error"),
    }
}

/// A definition id of the solver, standing for what it is. The ones the solver makes while it
/// works, which are numbered by the process, have no meaning that survives it.
fn definition(id: &SolverDefId<'_>, cx: &mut StableCx<'_>) {
    match id {
        SolverDefId::AdtId(id) => {
            cx.tag("adt id");
            let id: &AdtId = id;
            id.stable_hash(cx);
        }
        SolverDefId::ConstId(id) => {
            cx.tag("const id");
            id.stable_hash(cx);
        }
        SolverDefId::FunctionId(id) => {
            cx.tag("function id");
            id.stable_hash(cx);
        }
        SolverDefId::ImplId(id) => {
            cx.tag("impl id");
            id.stable_hash(cx);
        }
        SolverDefId::BuiltinDeriveImplId(id) => {
            cx.tag("builtin derive impl id");
            id.stable_hash(cx);
        }
        SolverDefId::StaticId(id) => {
            cx.tag("static id");
            id.stable_hash(cx);
        }
        SolverDefId::TraitId(id) => {
            cx.tag("trait id");
            id.stable_hash(cx);
        }
        SolverDefId::TypeAliasId(id) => {
            cx.tag("type alias id");
            id.stable_hash(cx);
        }
        SolverDefId::EnumVariantId(id) => {
            cx.tag("enum variant id");
            id.stable_hash(cx);
        }
        SolverDefId::Ctor(Ctor::Struct(id)) => {
            cx.tag("struct constructor");
            id.stable_hash(cx);
        }
        SolverDefId::Ctor(Ctor::Enum(id)) => {
            cx.tag("variant constructor");
            id.stable_hash(cx);
        }
        SolverDefId::AnonConstId(_) => cx.volatile("an anonymous constant the solver numbered"),
        SolverDefId::InternedClosureId(_) => cx.volatile("a closure the solver numbered"),
        SolverDefId::InternedCoroutineId(_) => cx.volatile("a coroutine the solver numbered"),
        SolverDefId::InternedCoroutineClosureId(_) => {
            cx.volatile("a coroutine closure the solver numbered")
        }
        SolverDefId::InternedOpaqueTyId(_) => cx.volatile("an opaque type the solver numbered"),
    }
}

impl StableHash for InherentImpls<'_> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { map } = self;
        cx.tag("inherent impls");
        cx.unordered(map.iter(), |cx, (self_ty, impls)| {
            simplified(self_ty, cx);
            impls.stable_hash(cx);
        });
    }
}

impl StableHash for OneTraitImpls<'_> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { non_blanket_impls, blanket_impls } = self;
        cx.unordered(non_blanket_impls.iter(), |cx, (self_ty, (impls, derived))| {
            simplified(self_ty, cx);
            impls.stable_hash(cx);
            derived.stable_hash(cx);
        });
        blanket_impls.stable_hash(cx);
    }
}

impl StableHash for TraitImpls<'_> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { map } = self;
        cx.tag("trait impls");
        cx.unordered(map.iter(), |cx, (trait_id, impls)| {
            trait_id.stable_hash(cx);
            impls.stable_hash(cx);
        });
    }
}

/// A hash of the inherent impls of `krate` that means the same in every process.
pub fn inherent_impls_fingerprint(db: &dyn HirDatabase, krate: Crate) -> StableFingerprint {
    let mut cx = StableCx::new(db);
    InherentImpls::for_crate(db, krate).stable_hash(&mut cx);
    cx.finish()
}

/// A hash of the trait impls of `krate` that means the same in every process.
pub fn trait_impls_fingerprint(db: &dyn HirDatabase, krate: Crate) -> StableFingerprint {
    let mut cx = StableCx::new(db);
    TraitImpls::for_crate(db, krate).stable_hash(&mut cx);
    cx.finish()
}
