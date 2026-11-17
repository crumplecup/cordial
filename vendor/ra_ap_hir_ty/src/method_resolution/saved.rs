//! The impl indexes of a crate, as they are saved with the database and restored by the next
//! process.
//!
//! What is saved is what is in them: the types impls are for, each as a `SimplifiedType`, and the
//! impls of each. The ids in them are persisted ids (see `hir_def`), which the restored database
//! gives the same numbers. A `SimplifiedType` is rustc's and has no serde, so it is written as a
//! `SavedSimplified`, matched exhaustively in both directions: a variant rust-analyzer adds is a
//! compile error here until it is given a form.
//!
//! The ids that the solver makes while it works (a closure, a coroutine, an opaque type, an
//! anonymous constant) are numbered by the process and cannot be saved. None can be the self type
//! of an impl, which is all these indexes hold; if one ever were, saving fails and says so, and
//! leaves nothing that restores as something else.

use hir_def::{
    AdtId, BuiltinDeriveImplId, ConstId, EnumVariantId, FunctionId, ImplId, StaticId, StructId,
    TraitId, TypeAliasId,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{InherentImpls, OneTraitImpls, TraitImpls};
use crate::next_solver::{Ctor, SimplifiedType, SolverDefId};

#[derive(Serialize, Deserialize)]
enum SavedDef {
    Adt(AdtId),
    Const(ConstId),
    Function(FunctionId),
    Impl(ImplId),
    BuiltinDeriveImpl(BuiltinDeriveImplId),
    Static(StaticId),
    Trait(TraitId),
    TypeAlias(TypeAliasId),
    EnumVariant(EnumVariantId),
    CtorStruct(StructId),
    CtorEnum(EnumVariantId),
}

impl SavedDef {
    fn of<E: serde::ser::Error>(def: &SolverDefId<'_>) -> Result<Self, E> {
        Ok(match *def {
            SolverDefId::AdtId(id) => Self::Adt(id),
            SolverDefId::ConstId(id) => Self::Const(id),
            SolverDefId::FunctionId(id) => Self::Function(id),
            SolverDefId::ImplId(id) => Self::Impl(id),
            SolverDefId::BuiltinDeriveImplId(id) => Self::BuiltinDeriveImpl(id),
            SolverDefId::StaticId(id) => Self::Static(id),
            SolverDefId::TraitId(id) => Self::Trait(id),
            SolverDefId::TypeAliasId(id) => Self::TypeAlias(id),
            SolverDefId::EnumVariantId(id) => Self::EnumVariant(id),
            SolverDefId::Ctor(Ctor::Struct(id)) => Self::CtorStruct(id),
            SolverDefId::Ctor(Ctor::Enum(id)) => Self::CtorEnum(id),
            SolverDefId::AnonConstId(_)
            | SolverDefId::InternedClosureId(_)
            | SolverDefId::InternedCoroutineId(_)
            | SolverDefId::InternedCoroutineClosureId(_)
            | SolverDefId::InternedOpaqueTyId(_) => {
                return Err(E::custom(
                    "an id the solver numbered itself is in an impl index, and has no meaning in \
                     another process",
                ));
            }
        })
    }

    fn to<'db>(&self) -> SolverDefId<'db> {
        match *self {
            Self::Adt(id) => SolverDefId::AdtId(id),
            Self::Const(id) => SolverDefId::ConstId(id),
            Self::Function(id) => SolverDefId::FunctionId(id),
            Self::Impl(id) => SolverDefId::ImplId(id),
            Self::BuiltinDeriveImpl(id) => SolverDefId::BuiltinDeriveImplId(id),
            Self::Static(id) => SolverDefId::StaticId(id),
            Self::Trait(id) => SolverDefId::TraitId(id),
            Self::TypeAlias(id) => SolverDefId::TypeAliasId(id),
            Self::EnumVariant(id) => SolverDefId::EnumVariantId(id),
            Self::CtorStruct(id) => SolverDefId::Ctor(Ctor::Struct(id)),
            Self::CtorEnum(id) => SolverDefId::Ctor(Ctor::Enum(id)),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum SavedSimplified {
    Bool,
    Char,
    Int(SavedInt),
    Uint(SavedUint),
    Float(SavedFloat),
    Adt(SavedDef),
    Foreign(SavedDef),
    Str,
    Array,
    Slice,
    Ref(bool),
    Ptr(bool),
    Never,
    Tuple(u64),
    MarkerTraitObject,
    Trait(SavedDef),
    Closure(SavedDef),
    Coroutine(SavedDef),
    CoroutineWitness(SavedDef),
    Function(u64),
    UnsafeBinder,
    Placeholder,
    Error,
}

#[derive(Serialize, Deserialize)]
enum SavedInt {
    Isize,
    I8,
    I16,
    I32,
    I64,
    I128,
}

#[derive(Serialize, Deserialize)]
enum SavedUint {
    Usize,
    U8,
    U16,
    U32,
    U64,
    U128,
}

#[derive(Serialize, Deserialize)]
enum SavedFloat {
    F16,
    F32,
    F64,
    F128,
}

impl SavedInt {
    fn of(int: rustc_type_ir::IntTy) -> Self {
        match int {
            rustc_type_ir::IntTy::Isize => Self::Isize,
            rustc_type_ir::IntTy::I8 => Self::I8,
            rustc_type_ir::IntTy::I16 => Self::I16,
            rustc_type_ir::IntTy::I32 => Self::I32,
            rustc_type_ir::IntTy::I64 => Self::I64,
            rustc_type_ir::IntTy::I128 => Self::I128,
        }
    }

    fn to(&self) -> rustc_type_ir::IntTy {
        match self {
            Self::Isize => rustc_type_ir::IntTy::Isize,
            Self::I8 => rustc_type_ir::IntTy::I8,
            Self::I16 => rustc_type_ir::IntTy::I16,
            Self::I32 => rustc_type_ir::IntTy::I32,
            Self::I64 => rustc_type_ir::IntTy::I64,
            Self::I128 => rustc_type_ir::IntTy::I128,
        }
    }
}

impl SavedUint {
    fn of(uint: rustc_type_ir::UintTy) -> Self {
        match uint {
            rustc_type_ir::UintTy::Usize => Self::Usize,
            rustc_type_ir::UintTy::U8 => Self::U8,
            rustc_type_ir::UintTy::U16 => Self::U16,
            rustc_type_ir::UintTy::U32 => Self::U32,
            rustc_type_ir::UintTy::U64 => Self::U64,
            rustc_type_ir::UintTy::U128 => Self::U128,
        }
    }

    fn to(&self) -> rustc_type_ir::UintTy {
        match self {
            Self::Usize => rustc_type_ir::UintTy::Usize,
            Self::U8 => rustc_type_ir::UintTy::U8,
            Self::U16 => rustc_type_ir::UintTy::U16,
            Self::U32 => rustc_type_ir::UintTy::U32,
            Self::U64 => rustc_type_ir::UintTy::U64,
            Self::U128 => rustc_type_ir::UintTy::U128,
        }
    }
}

impl SavedFloat {
    fn of(float: rustc_type_ir::FloatTy) -> Self {
        match float {
            rustc_type_ir::FloatTy::F16 => Self::F16,
            rustc_type_ir::FloatTy::F32 => Self::F32,
            rustc_type_ir::FloatTy::F64 => Self::F64,
            rustc_type_ir::FloatTy::F128 => Self::F128,
        }
    }

    fn to(&self) -> rustc_type_ir::FloatTy {
        match self {
            Self::F16 => rustc_type_ir::FloatTy::F16,
            Self::F32 => rustc_type_ir::FloatTy::F32,
            Self::F64 => rustc_type_ir::FloatTy::F64,
            Self::F128 => rustc_type_ir::FloatTy::F128,
        }
    }
}

fn is_mut(mutability: rustc_ast_ir::Mutability) -> bool {
    match mutability {
        rustc_ast_ir::Mutability::Not => false,
        rustc_ast_ir::Mutability::Mut => true,
    }
}

fn mutability(is_mut: bool) -> rustc_ast_ir::Mutability {
    if is_mut { rustc_ast_ir::Mutability::Mut } else { rustc_ast_ir::Mutability::Not }
}

impl SavedSimplified {
    fn of<E: serde::ser::Error>(ty: &SimplifiedType<'_>) -> Result<Self, E> {
        Ok(match ty {
            SimplifiedType::Bool => Self::Bool,
            SimplifiedType::Char => Self::Char,
            SimplifiedType::Int(int) => Self::Int(SavedInt::of(*int)),
            SimplifiedType::Uint(uint) => Self::Uint(SavedUint::of(*uint)),
            SimplifiedType::Float(float) => Self::Float(SavedFloat::of(*float)),
            SimplifiedType::Adt(def) => Self::Adt(SavedDef::of(def)?),
            SimplifiedType::Foreign(def) => Self::Foreign(SavedDef::of(def)?),
            SimplifiedType::Str => Self::Str,
            SimplifiedType::Array => Self::Array,
            SimplifiedType::Slice => Self::Slice,
            SimplifiedType::Ref(mutability) => Self::Ref(is_mut(*mutability)),
            SimplifiedType::Ptr(mutability) => Self::Ptr(is_mut(*mutability)),
            SimplifiedType::Never => Self::Never,
            SimplifiedType::Tuple(arity) => Self::Tuple(*arity as u64),
            SimplifiedType::MarkerTraitObject => Self::MarkerTraitObject,
            SimplifiedType::Trait(def) => Self::Trait(SavedDef::of(def)?),
            SimplifiedType::Closure(def) => Self::Closure(SavedDef::of(def)?),
            SimplifiedType::Coroutine(def) => Self::Coroutine(SavedDef::of(def)?),
            SimplifiedType::CoroutineWitness(def) => Self::CoroutineWitness(SavedDef::of(def)?),
            SimplifiedType::Function(arity) => Self::Function(*arity as u64),
            SimplifiedType::UnsafeBinder => Self::UnsafeBinder,
            SimplifiedType::Placeholder => Self::Placeholder,
            SimplifiedType::Error => Self::Error,
        })
    }

    fn to<'db, E: serde::de::Error>(&self) -> Result<SimplifiedType<'db>, E> {
        let arity = |arity: u64| {
            usize::try_from(arity).map_err(|_| E::custom("an arity that is not a number here"))
        };
        Ok(match self {
            Self::Bool => SimplifiedType::Bool,
            Self::Char => SimplifiedType::Char,
            Self::Int(int) => SimplifiedType::Int(int.to()),
            Self::Uint(uint) => SimplifiedType::Uint(uint.to()),
            Self::Float(float) => SimplifiedType::Float(float.to()),
            Self::Adt(def) => SimplifiedType::Adt(def.to()),
            Self::Foreign(def) => SimplifiedType::Foreign(def.to()),
            Self::Str => SimplifiedType::Str,
            Self::Array => SimplifiedType::Array,
            Self::Slice => SimplifiedType::Slice,
            Self::Ref(is_mut) => SimplifiedType::Ref(mutability(*is_mut)),
            Self::Ptr(is_mut) => SimplifiedType::Ptr(mutability(*is_mut)),
            Self::Never => SimplifiedType::Never,
            Self::Tuple(count) => SimplifiedType::Tuple(arity(*count)?),
            Self::MarkerTraitObject => SimplifiedType::MarkerTraitObject,
            Self::Trait(def) => SimplifiedType::Trait(def.to()),
            Self::Closure(def) => SimplifiedType::Closure(def.to()),
            Self::Coroutine(def) => SimplifiedType::Coroutine(def.to()),
            Self::CoroutineWitness(def) => SimplifiedType::CoroutineWitness(def.to()),
            Self::Function(count) => SimplifiedType::Function(arity(*count)?),
            Self::UnsafeBinder => SimplifiedType::UnsafeBinder,
            Self::Placeholder => SimplifiedType::Placeholder,
            Self::Error => SimplifiedType::Error,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct SavedInherent {
    map: Vec<(SavedSimplified, Vec<ImplId>)>,
}

impl Serialize for InherentImpls<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Self { map } = self;
        let map = map
            .iter()
            .map(|(self_ty, impls)| Ok((SavedSimplified::of(self_ty)?, impls.to_vec())))
            .collect::<Result<Vec<_>, S::Error>>()?;
        SavedInherent { map }.serialize(serializer)
    }
}

impl<'de, 'db> Deserialize<'de> for InherentImpls<'db> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedInherent::deserialize(deserializer)?;
        let map = saved
            .map
            .into_iter()
            .map(|(self_ty, impls)| Ok((self_ty.to::<D::Error>()?, impls.into_boxed_slice())))
            .collect::<Result<_, D::Error>>()?;
        Ok(Self { map })
    }
}

#[derive(Serialize, Deserialize)]
struct SavedOneTrait {
    non_blanket: Vec<(SavedSimplified, Vec<ImplId>, Vec<BuiltinDeriveImplId>)>,
    blanket: Vec<ImplId>,
}

#[derive(Serialize, Deserialize)]
struct SavedTrait {
    map: Vec<(TraitId, SavedOneTrait)>,
}

impl Serialize for TraitImpls<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Self { map } = self;
        let map = map
            .iter()
            .map(|(trait_id, one)| {
                let OneTraitImpls { non_blanket_impls, blanket_impls } = one;
                let non_blanket = non_blanket_impls
                    .iter()
                    .map(|(self_ty, (impls, derived))| {
                        Ok((SavedSimplified::of(self_ty)?, impls.to_vec(), derived.to_vec()))
                    })
                    .collect::<Result<Vec<_>, S::Error>>()?;
                Ok((*trait_id, SavedOneTrait { non_blanket, blanket: blanket_impls.to_vec() }))
            })
            .collect::<Result<Vec<_>, S::Error>>()?;
        SavedTrait { map }.serialize(serializer)
    }
}

impl<'de, 'db> Deserialize<'de> for TraitImpls<'db> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedTrait::deserialize(deserializer)?;
        let map = saved
            .map
            .into_iter()
            .map(|(trait_id, one)| {
                let non_blanket_impls = one
                    .non_blanket
                    .into_iter()
                    .map(|(self_ty, impls, derived)| {
                        Ok((
                            self_ty.to::<D::Error>()?,
                            (impls.into_boxed_slice(), derived.into_boxed_slice()),
                        ))
                    })
                    .collect::<Result<_, D::Error>>()?;
                Ok((
                    trait_id,
                    OneTraitImpls { non_blanket_impls, blanket_impls: one.blanket.into_boxed_slice() },
                ))
            })
            .collect::<Result<_, D::Error>>()?;
        Ok(Self { map })
    }
}
