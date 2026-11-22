//! Argument-based retention for context-local composite queries.
//!
//! This walks stored identities, never the analysis needed to evaluate them.
//! Ordinary crate IDs are shared; only roots explicitly registered by the loader
//! govern retention. The visitor's sets are temporary DAG traversal state.

use base_db::Crate;
use hir_def::HasModule;
use rustc_hash::FxHashSet;
use rustc_type_ir::{
    BoundTyKind, BoundVariableKind, TypeSuperVisitable, TypeVisitable, TypeVisitor,
    inherent::IntoKind,
};
use salsa::{
    Id, QueryOwnership,
    plumbing::{AsId, FromId},
};

use crate::{
    ImplTraitId,
    db::HirDatabase,
    mir::{MonomorphizedMirBodyKey, monomorphized_mir_body_query},
    next_solver::{
        Binder, BoundRegionKind, ClauseKind, Const, ConstKind, Ctor, DbInterner, Predicate,
        PredicateKind, Region, RegionKind, SolverDefId, Ty, TyKind,
    },
    traits::StoredParamEnvAndCrate,
};

/// Install the opted-in policies once after database construction or restoration.
/// Restored callbacks are registered independently of persisted declarations.
pub fn register(db: &mut impl salsa::Database) {
    hir_expand::register_query_ownership(db);
    crate::lower::register_query_ownership(db);
    crate::specialization::register_query_ownership(db);
    salsa::register_query_ownership::<monomorphized_mir_body_query, dyn HirDatabase>(
        db, mir_owners,
    );
    salsa::register_query_ownership::<
        crate::mir::monomorphized_mir_body_for_closure_query,
        dyn HirDatabase,
    >(db, closure_owners);
    salsa::register_query_ownership::<crate::consteval::const_eval_query, dyn HirDatabase>(
        db,
        const_owners,
    );
    salsa::register_query_ownership::<crate::consteval::anon_const_eval_query, dyn HirDatabase>(
        db,
        anon_const_owners,
    );
    salsa::register_query_ownership::<crate::layout::layout_of_ty_query, dyn HirDatabase>(
        db,
        type_layout_owners,
    );
    salsa::register_query_ownership::<crate::layout::layout_of_adt_query, dyn HirDatabase>(
        db,
        adt_layout_owners,
    );
}

#[hotpath::measure]
fn mir_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = MonomorphizedMirBodyKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.definition(key.owner(db));
    owners.environment(key.trait_env(db));
    key.subst(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

#[hotpath::measure]
fn closure_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = crate::mir::MonomorphizedClosureKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.definition(key.closure(db).loc(db).owner);
    owners.environment(key.trait_env(db));
    key.subst(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

#[hotpath::measure]
fn const_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = crate::consteval::ConstEvalKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.definition(key.def(db));
    if let Some(environment) = key.trait_env(db) {
        owners.environment(environment);
    }
    key.subst(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

#[hotpath::measure]
fn anon_const_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = crate::consteval::AnonConstEvalKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.def_id(key.def(db).into());
    if let Some(environment) = key.trait_env(db) {
        owners.environment(environment);
    }
    key.subst(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

#[hotpath::measure]
fn type_layout_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = crate::layout::TypeLayoutKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.environment(key.trait_env(db));
    key.ty(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

#[hotpath::measure]
fn adt_layout_owners(db: &dyn HirDatabase, id: Id) -> QueryOwnership {
    let key = crate::layout::AdtLayoutKey::from_id(id);
    let mut owners = ArgumentOwners::new(db);
    owners.definition(key.def(db));
    owners.environment(key.trait_env(db));
    key.args(db).as_ref().visit_with(&mut owners);
    owners.finish()
}

pub(crate) struct ArgumentOwners<'db> {
    db: &'db dyn HirDatabase,
    roots: Vec<Id>,
    types: FxHashSet<Ty<'db>>,
    consts: FxHashSet<Const<'db>>,
    predicates: FxHashSet<Predicate<'db>>,
    definitions: FxHashSet<SolverDefId<'db>>,
    unclassified: bool,
}

impl<'db> ArgumentOwners<'db> {
    pub(crate) fn new(db: &'db dyn HirDatabase) -> Self {
        Self {
            db,
            roots: Vec::new(),
            types: FxHashSet::default(),
            consts: FxHashSet::default(),
            predicates: FxHashSet::default(),
            definitions: FxHashSet::default(),
            unclassified: false,
        }
    }

    pub(crate) fn finish(self) -> QueryOwnership {
        if self.unclassified {
            QueryOwnership::Unclassified
        } else {
            QueryOwnership::owned(self.roots)
        }
    }

    pub(crate) fn krate(&mut self, krate: Crate) {
        let id = krate.as_id();
        if salsa::is_owner_root(self.db, id) {
            self.roots.push(id);
        }
    }

    pub(crate) fn definition(&mut self, definition: impl HasModule) {
        self.krate(definition.krate(self.db));
    }

    pub(crate) fn environment(&mut self, environment: &StoredParamEnvAndCrate) {
        self.krate(environment.krate);
        environment.param_env(self.db).clauses.visit_with(self);
    }

    fn def_id(&mut self, id: SolverDefId<'db>) {
        if !self.definitions.insert(id) {
            return;
        }
        match id {
            SolverDefId::AdtId(id) => self.definition(id),
            SolverDefId::ConstId(id) => self.definition(id),
            SolverDefId::FunctionId(id) => self.definition(id),
            SolverDefId::ImplId(id) => self.definition(id),
            SolverDefId::BuiltinDeriveImplId(id) => self.definition(id),
            SolverDefId::StaticId(id) => self.definition(id),
            SolverDefId::AnonConstId(id) => {
                self.definition(id);
                id.loc(self.db)
                    .ty
                    .as_ref()
                    .skip_binder()
                    .as_ref()
                    .visit_with(self);
            }
            SolverDefId::TraitId(id) => self.definition(id),
            SolverDefId::TypeAliasId(id) => self.definition(id),
            SolverDefId::InternedClosureId(id) => self.definition(id.loc(self.db).owner),
            SolverDefId::InternedCoroutineId(id) => self.definition(id.loc(self.db).owner),
            SolverDefId::InternedCoroutineClosureId(id) => self.definition(id.loc(self.db).owner),
            SolverDefId::InternedOpaqueTyId(id) => match id.loc(self.db) {
                ImplTraitId::ReturnTypeImplTrait(id, _) => self.definition(id),
                ImplTraitId::TypeAliasImplTrait(id, _) => self.definition(id),
            },
            SolverDefId::EnumVariantId(id) | SolverDefId::Ctor(Ctor::Enum(id)) => {
                self.definition(id)
            }
            SolverDefId::Ctor(Ctor::Struct(id)) => self.definition(id),
        }
    }

    fn bound_region(&mut self, kind: BoundRegionKind<'db>) {
        match kind {
            BoundRegionKind::Named(id) => self.def_id(id),
            BoundRegionKind::Anon
            | BoundRegionKind::NamedForPrinting(_)
            | BoundRegionKind::ClosureEnv => {}
        }
    }

    fn bound_type(&mut self, kind: BoundTyKind<DbInterner<'db>>) {
        match kind {
            BoundTyKind::Param(id) => self.def_id(id),
            BoundTyKind::Anon => {}
        }
    }
}

impl<'db> TypeVisitor<DbInterner<'db>> for ArgumentOwners<'db> {
    type Result = ();

    fn visit_binder<T: TypeVisitable<DbInterner<'db>>>(&mut self, binder: &Binder<'db, T>) {
        for variable in binder.bound_vars().iter() {
            match variable {
                BoundVariableKind::Ty(kind) => self.bound_type(kind),
                BoundVariableKind::Region(kind) => self.bound_region(kind),
                BoundVariableKind::Const => {}
            }
        }
        binder.super_visit_with(self);
    }

    fn visit_ty(&mut self, ty: Ty<'db>) {
        if !self.types.insert(ty) {
            return;
        }
        // The standard type visitor deliberately skips definition IDs. Collect
        // those fields explicitly, then let it recurse through nested arguments.
        match ty.kind() {
            TyKind::Adt(def, _) => self.definition(def.def_id()),
            TyKind::Foreign(id) => self.definition(id.0),
            TyKind::FnDef(id, _) => self.def_id(id.into()),
            TyKind::Closure(id, _) => self.def_id(id.into()),
            TyKind::CoroutineClosure(id, _) => self.def_id(id.into()),
            TyKind::Coroutine(id, _) | TyKind::CoroutineWitness(id, _) => self.def_id(id.into()),
            TyKind::Alias(alias) => self.def_id(alias.kind.def_id()),
            TyKind::Param(param) => self.definition(param.id.parent()),
            TyKind::Bound(_, bound) => self.bound_type(bound.kind),
            TyKind::Placeholder(placeholder) => self.bound_type(placeholder.bound.kind),
            TyKind::Dynamic(predicates, _) => {
                for predicate in predicates.iter() {
                    self.def_id(predicate.def_id());
                }
            }
            TyKind::Infer(_) => self.unclassified = true,
            TyKind::Bool
            | TyKind::Char
            | TyKind::Int(_)
            | TyKind::Uint(_)
            | TyKind::Float(_)
            | TyKind::Str
            | TyKind::Array(..)
            | TyKind::Pat(..)
            | TyKind::Slice(_)
            | TyKind::RawPtr(..)
            | TyKind::Ref(..)
            | TyKind::FnPtr(..)
            | TyKind::UnsafeBinder(_)
            | TyKind::Never
            | TyKind::Tuple(_)
            | TyKind::Error(_) => {}
        }
        ty.super_visit_with(self);
    }

    fn visit_const(&mut self, constant: Const<'db>) {
        if !self.consts.insert(constant) {
            return;
        }
        match constant.kind() {
            ConstKind::Param(param) => self.definition(param.id.parent()),
            ConstKind::Unevaluated(value) => self.def_id(value.def.into()),
            ConstKind::Infer(_) => self.unclassified = true,
            ConstKind::Bound(..)
            | ConstKind::Placeholder(_)
            | ConstKind::Value(_)
            | ConstKind::Error(_)
            | ConstKind::Expr(_) => {}
        }
        constant.super_visit_with(self);
    }

    fn visit_region(&mut self, region: Region<'db>) {
        match region.kind() {
            RegionKind::ReEarlyParam(param) => self.definition(param.id.parent),
            RegionKind::ReBound(_, bound) => self.bound_region(bound.kind),
            RegionKind::ReLateParam(param) => {
                self.def_id(param.scope);
                self.bound_region(param.bound_region.kind);
            }
            RegionKind::RePlaceholder(placeholder) => self.bound_region(placeholder.bound.kind),
            RegionKind::ReVar(_) => self.unclassified = true,
            RegionKind::ReStatic | RegionKind::ReErased | RegionKind::ReError(_) => {}
        }
    }

    fn visit_predicate(&mut self, predicate: Predicate<'db>) {
        if !self.predicates.insert(predicate) {
            return;
        }
        match predicate.kind().skip_binder() {
            PredicateKind::Clause(clause) => match clause {
                ClauseKind::Trait(predicate) => self.def_id(predicate.trait_ref.def_id.into()),
                ClauseKind::Projection(predicate) => {
                    self.def_id(predicate.projection_term.def_id())
                }
                ClauseKind::HostEffect(predicate) => self.def_id(predicate.trait_ref.def_id.into()),
                ClauseKind::RegionOutlives(_)
                | ClauseKind::TypeOutlives(_)
                | ClauseKind::ConstArgHasType(..)
                | ClauseKind::WellFormed(_)
                | ClauseKind::ConstEvaluatable(_)
                | ClauseKind::UnstableFeature(_) => {}
            },
            PredicateKind::DynCompatible(id) => self.def_id(id.into()),
            PredicateKind::NormalizesTo(predicate) => self.def_id(predicate.alias.def_id()),
            PredicateKind::Subtype(_)
            | PredicateKind::Coerce(_)
            | PredicateKind::ConstEquate(..)
            | PredicateKind::Ambiguous
            | PredicateKind::AliasRelate(..) => {}
        }
        predicate.super_visit_with(self);
    }
}
