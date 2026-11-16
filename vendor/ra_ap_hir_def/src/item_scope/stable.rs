//! What an item scope says, hashed so that it means the same in every process. See
//! `hir_expand::stable`.
//!
//! Every struct and enum here is taken apart without `..`: a field rust-analyzer adds to a scope is
//! a compile error in this file until someone says what it adds to what a dependent can read.

use hir_expand::stable::{StableCx, StableHash};

use super::{
    DeriveMacroInvocation, GlobId, ImportId, ImportOrDef, ImportOrExternCrate, ImportOrGlob,
    ItemScope,
};
use crate::per_ns::Item;

impl StableHash for ImportId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { use_, idx } = self;
        use_.stable_hash(cx);
        // Which `use` tree of the declaration: a position in it.
        cx.write_plain(idx);
    }
}

impl StableHash for GlobId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { use_, idx } = self;
        use_.stable_hash(cx);
        cx.write_plain(idx);
    }
}

impl StableHash for ImportOrExternCrate {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ImportOrExternCrate::Glob(id) => {
                cx.tag("glob");
                id.stable_hash(cx);
            }
            ImportOrExternCrate::Import(id) => {
                cx.tag("import");
                id.stable_hash(cx);
            }
            ImportOrExternCrate::ExternCrate(id) => {
                cx.tag("extern crate");
                id.stable_hash(cx);
            }
        }
    }
}

impl StableHash for ImportOrGlob {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ImportOrGlob::Glob(id) => {
                cx.tag("glob");
                id.stable_hash(cx);
            }
            ImportOrGlob::Import(id) => {
                cx.tag("import");
                id.stable_hash(cx);
            }
        }
    }
}

impl StableHash for ImportOrDef {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ImportOrDef::Import(id) => {
                cx.tag("import");
                id.stable_hash(cx);
            }
            ImportOrDef::Glob(id) => {
                cx.tag("glob");
                id.stable_hash(cx);
            }
            ImportOrDef::ExternCrate(id) => {
                cx.tag("extern crate");
                id.stable_hash(cx);
            }
            ImportOrDef::Def(def) => {
                cx.tag("definition");
                def.stable_hash(cx);
            }
        }
    }
}

impl<Def: StableHash, Import: StableHash> StableHash for Item<Def, Import> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { def, vis, import } = self;
        def.stable_hash(cx);
        vis.stable_hash(cx);
        import.stable_hash(cx);
    }
}

impl StableHash for DeriveMacroInvocation {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { attr_id, attr_call_id, derive_call_ids } = self;
        attr_id.stable_hash(cx);
        attr_call_id.stable_hash(cx);
        cx.write_u64(derive_call_ids.len() as u64);
        for call in derive_call_ids.iter() {
            call.stable_hash(cx);
        }
    }
}

impl StableHash for ItemScope {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self {
            types,
            values,
            macros,
            unresolved,
            declarations,
            impls,
            builtin_derive_impls,
            extern_blocks,
            unnamed_consts,
            unnamed_trait_imports,
            use_imports_types,
            use_imports_values,
            use_imports_macros,
            use_decls,
            extern_crate_decls,
            legacy_macros,
            attr_macros,
            macro_invocations,
            derive_macros,
        } = self;
        cx.tag("item scope");
        // The names in a namespace are a map: it does not matter what order they were found in.
        cx.unordered(types.iter(), |cx, (name, item)| {
            name.stable_hash(cx);
            item.stable_hash(cx);
        });
        cx.unordered(values.iter(), |cx, (name, item)| {
            name.stable_hash(cx);
            item.stable_hash(cx);
        });
        cx.unordered(macros.iter(), |cx, (name, item)| {
            name.stable_hash(cx);
            item.stable_hash(cx);
        });
        cx.unordered(unresolved.iter(), |cx, name| name.stable_hash(cx));
        declarations.stable_hash(cx);
        impls.stable_hash(cx);
        builtin_derive_impls.stable_hash(cx);
        extern_blocks.stable_hash(cx);
        unnamed_consts.stable_hash(cx);
        unnamed_trait_imports.stable_hash(cx);
        cx.unordered(use_imports_types.iter(), |cx, (import, resolved)| {
            import.stable_hash(cx);
            resolved.stable_hash(cx);
        });
        cx.unordered(use_imports_values.iter(), |cx, (import, resolved)| {
            import.stable_hash(cx);
            resolved.stable_hash(cx);
        });
        cx.unordered(use_imports_macros.iter(), |cx, (import, resolved)| {
            import.stable_hash(cx);
            resolved.stable_hash(cx);
        });
        use_decls.stable_hash(cx);
        extern_crate_decls.stable_hash(cx);
        cx.unordered(legacy_macros.iter(), |cx, (name, macros)| {
            name.stable_hash(cx);
            cx.write_u64(macros.len() as u64);
            for id in macros {
                id.stable_hash(cx);
            }
        });
        cx.unordered(attr_macros.iter(), |cx, (item, call)| {
            item.stable_hash(cx);
            call.stable_hash(cx);
        });
        cx.unordered(macro_invocations.iter(), |cx, (call_site, call)| {
            call_site.stable_hash(cx);
            call.stable_hash(cx);
        });
        cx.unordered(derive_macros.iter(), |cx, (adt, invocations)| {
            adt.stable_hash(cx);
            cx.write_u64(invocations.len() as u64);
            for invocation in invocations {
                invocation.stable_hash(cx);
            }
        });
    }
}
