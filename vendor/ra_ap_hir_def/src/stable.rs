//! What the ids of a definition map stand for, hashed so that it means the same in every process.
//! See `hir_expand::stable` for why, and for the two rules.
//!
//! An id is hashed by its location: the container it is in and where it is written. Every impl
//! destructures what it hashes without `..`, so a field rust-analyzer adds later is a compile
//! error here until someone says what it adds.

use hir_expand::stable::{StableCx, StableHash};

use crate::{
    AdtId, AssocItemLoc, BlockId, BuiltinDeriveImplId, BuiltinDeriveImplLoc, EnumVariantLoc,
    ItemContainerId, ItemLoc, Macro2Loc, MacroId, MacroRulesLoc, ModuleDefId, ModuleId,
    ProcMacroLoc, visibility::Visibility,
};
use span::AstIdNode;

impl<N: AstIdNode> StableHash for ItemLoc<N> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { container, id } = self;
        container.stable_hash(cx);
        id.stable_hash(cx);
    }
}

impl<N: AstIdNode> StableHash for AssocItemLoc<N> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { container, id } = self;
        container.stable_hash(cx);
        id.stable_hash(cx);
    }
}

impl StableHash for EnumVariantLoc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { id, parent, name } = self;
        id.stable_hash(cx);
        parent.stable_hash(cx);
        name.stable_hash(cx);
    }
}

impl StableHash for Macro2Loc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { container, id, expander, allow_internal_unsafe, edition } = self;
        container.stable_hash(cx);
        id.stable_hash(cx);
        cx.write_plain(expander);
        allow_internal_unsafe.stable_hash(cx);
        cx.write_plain(edition);
    }
}

impl StableHash for MacroRulesLoc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { container, id, expander, flags, edition } = self;
        container.stable_hash(cx);
        id.stable_hash(cx);
        cx.write_plain(expander);
        cx.write_plain(&flags.bits());
        cx.write_plain(edition);
    }
}

impl StableHash for ProcMacroLoc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { container, id, expander, kind, edition } = self;
        container.stable_hash(cx);
        id.stable_hash(cx);
        cx.write_plain(expander);
        cx.write_plain(kind);
        cx.write_plain(edition);
    }
}

impl StableHash for BuiltinDeriveImplLoc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { adt, trait_, derive_attr_id, derive_index } = self;
        adt.stable_hash(cx);
        cx.write_plain(trait_);
        derive_attr_id.stable_hash(cx);
        derive_index.stable_hash(cx);
    }
}

impl StableHash for BuiltinDeriveImplId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        self.loc(cx.db).stable_hash(cx);
    }
}

impl StableHash for ModuleId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // A module is its crate, the block it is in if it is in one, its name and where it is.
        cx.tag("module");
        self.krate(cx.db).stable_hash(cx);
        // SAFETY: it is hashed while the query that made it is still running, and not kept.
        self.block(cx.db).map(|block| unsafe { block.to_static() }).stable_hash(cx);
        self.name_or_empty(cx.db).stable_hash(cx);
        // SAFETY: it is hashed while the query that made it is still running, and not kept.
        self.containing_module_inside_def_map(cx.db)
            .map(|parent| unsafe { parent.to_static() })
            .stable_hash(cx);
    }
}

impl StableHash for BlockId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.tag("block");
        self.ast_id(cx.db).stable_hash(cx);
        // SAFETY: as above.
        unsafe { self.module(cx.db).to_static() }.stable_hash(cx);
    }
}

impl StableHash for ItemContainerId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ItemContainerId::ExternBlockId(id) => {
                cx.tag("in an extern block");
                id.stable_hash(cx);
            }
            ItemContainerId::ModuleId(id) => {
                cx.tag("in a module");
                id.stable_hash(cx);
            }
            ItemContainerId::ImplId(id) => {
                cx.tag("in an impl");
                id.stable_hash(cx);
            }
            ItemContainerId::TraitId(id) => {
                cx.tag("in a trait");
                id.stable_hash(cx);
            }
        }
    }
}

impl StableHash for AdtId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            AdtId::StructId(id) => {
                cx.tag("struct");
                id.stable_hash(cx);
            }
            AdtId::UnionId(id) => {
                cx.tag("union");
                id.stable_hash(cx);
            }
            AdtId::EnumId(id) => {
                cx.tag("enum");
                id.stable_hash(cx);
            }
        }
    }
}

impl StableHash for MacroId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            MacroId::Macro2Id(id) => {
                cx.tag("macro 2.0");
                id.stable_hash(cx);
            }
            MacroId::MacroRulesId(id) => {
                cx.tag("macro_rules");
                id.stable_hash(cx);
            }
            MacroId::ProcMacroId(id) => {
                cx.tag("proc macro");
                id.stable_hash(cx);
            }
        }
    }
}

impl StableHash for ModuleDefId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ModuleDefId::ModuleId(id) => id.stable_hash(cx),
            ModuleDefId::FunctionId(id) => {
                cx.tag("function");
                id.stable_hash(cx);
            }
            ModuleDefId::AdtId(id) => id.stable_hash(cx),
            ModuleDefId::EnumVariantId(id) => {
                cx.tag("variant");
                id.stable_hash(cx);
            }
            ModuleDefId::ConstId(id) => {
                cx.tag("const");
                id.stable_hash(cx);
            }
            ModuleDefId::StaticId(id) => {
                cx.tag("static");
                id.stable_hash(cx);
            }
            ModuleDefId::TraitId(id) => {
                cx.tag("trait");
                id.stable_hash(cx);
            }
            ModuleDefId::TypeAliasId(id) => {
                cx.tag("type alias");
                id.stable_hash(cx);
            }
            ModuleDefId::BuiltinType(builtin) => {
                cx.tag("builtin type");
                cx.write_plain(builtin);
            }
            ModuleDefId::MacroId(id) => id.stable_hash(cx),
        }
    }
}

impl StableHash for Visibility {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            Visibility::Module(module, explicitness) => {
                cx.tag("visible in a module");
                module.stable_hash(cx);
                cx.write_plain(explicitness);
            }
            Visibility::PubCrate(krate) => {
                cx.tag("visible in a crate");
                krate.stable_hash(cx);
            }
            Visibility::Public => cx.tag("public"),
        }
    }
}
