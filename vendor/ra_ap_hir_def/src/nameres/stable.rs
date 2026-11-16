//! What a definition map says, hashed so that it means the same in every process.
//! See `hir_expand::stable` for why, and for the two rules.
//!
//! A definition map is equal to another when everything in it is, with ids compared by what they
//! stand for. This hashes exactly that: it takes the map apart without `..` at every level, so a
//! field rust-analyzer adds later is a compile error here until someone says what it adds, and an
//! omission is something written down at the place it is made, with its reason.

use hir_expand::stable::{StableCx, StableFingerprint, StableHash};

use super::{
    BlockInfo, DefMap, DefMapCrateData, LocalDefMap, ModuleData, ModuleOrigin, ModulesMap,
    diagnostics::{DefDiagnostic, DefDiagnosticKind},
};
use crate::unstable_features::UnstableFeatures;
use base_db::Crate;

impl StableHash for ModuleOrigin {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            ModuleOrigin::CrateRoot { definition } => {
                cx.tag("crate root");
                definition.stable_hash(cx);
            }
            ModuleOrigin::File { is_mod_rs, declaration, declaration_tree_id, definition } => {
                cx.tag("file");
                is_mod_rs.stable_hash(cx);
                declaration.stable_hash(cx);
                declaration_tree_id.stable_hash(cx);
                definition.stable_hash(cx);
            }
            ModuleOrigin::Inline { definition_tree_id, definition } => {
                cx.tag("inline");
                definition_tree_id.stable_hash(cx);
                definition.stable_hash(cx);
            }
            ModuleOrigin::BlockExpr { id, block } => {
                cx.tag("block expression");
                id.stable_hash(cx);
                block.stable_hash(cx);
            }
        }
    }
}

impl StableHash for ModuleData {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { origin, visibility, parent, children, scope } = self;
        origin.stable_hash(cx);
        visibility.stable_hash(cx);
        parent.stable_hash(cx);
        cx.unordered(children.iter(), |cx, (name, child)| {
            name.stable_hash(cx);
            child.stable_hash(cx);
        });
        scope.stable_hash(cx);
    }
}

impl StableHash for ModulesMap {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { inner } = self;
        cx.unordered(inner.iter(), |cx, (id, data)| {
            id.stable_hash(cx);
            data.stable_hash(cx);
        });
    }
}

impl StableHash for BlockInfo {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { block, parent } = self;
        block.stable_hash(cx);
        parent.stable_hash(cx);
    }
}

impl StableHash for UnstableFeatures {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // Two sets of features are equal when the features named are (`==` compares nothing else).
        cx.unordered(self.iter(), |cx, feature| cx.write_str(feature.as_str()));
    }
}

impl StableHash for DefMapCrateData {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self {
            exported_derives,
            fn_proc_macro_mapping,
            fn_proc_macro_mapping_back,
            registered_tools,
            unstable_features,
            rustc_coherence_is_core,
            no_core,
            no_std,
            edition,
            recursion_limit,
        } = self;
        cx.unordered(exported_derives.iter(), |cx, (macro_id, helpers)| {
            macro_id.stable_hash(cx);
            cx.write_u64(helpers.len() as u64);
            for helper in helpers.iter() {
                helper.stable_hash(cx);
            }
        });
        cx.unordered(fn_proc_macro_mapping.iter(), |cx, (function, proc_macro)| {
            function.stable_hash(cx);
            proc_macro.stable_hash(cx);
        });
        cx.unordered(fn_proc_macro_mapping_back.iter(), |cx, (proc_macro, function)| {
            proc_macro.stable_hash(cx);
            function.stable_hash(cx);
        });
        cx.write_u64(registered_tools.len() as u64);
        for tool in registered_tools {
            cx.write_str(tool.as_str());
        }
        unstable_features.stable_hash(cx);
        rustc_coherence_is_core.stable_hash(cx);
        no_core.stable_hash(cx);
        no_std.stable_hash(cx);
        cx.write_plain(edition);
        cx.write_plain(recursion_limit);
    }
}

impl StableHash for DefDiagnostic {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { in_module, kind } = self;
        in_module.stable_hash(cx);
        // What a diagnostic says is for whoever shows diagnostics: nothing that computes what a
        // function does reads it. Which kind it is and where it is are hashed, so a diagnostic
        // appearing or going shows; the text, paths and configuration inside it are named here and
        // left out, each at its own variant.
        match kind {
            DefDiagnosticKind::UnresolvedModule { ast, candidates: _candidates } => {
                cx.tag("unresolved module");
                ast.stable_hash(cx);
            }
            DefDiagnosticKind::UnresolvedExternCrate { ast } => {
                cx.tag("unresolved extern crate");
                ast.stable_hash(cx);
            }
            DefDiagnosticKind::UnresolvedImport { id, index } => {
                cx.tag("unresolved import");
                id.stable_hash(cx);
                cx.write_plain(index);
            }
            DefDiagnosticKind::UnconfiguredCode { ast_id, cfg: _cfg, opts: _opts } => {
                cx.tag("unconfigured code");
                ast_id.stable_hash(cx);
            }
            DefDiagnosticKind::UnresolvedMacroCall { ast, path: _path } => {
                cx.tag("unresolved macro call");
                ast.stable_hash(cx);
            }
            DefDiagnosticKind::UnimplementedBuiltinMacro { ast } => {
                cx.tag("unimplemented built-in macro");
                ast.stable_hash(cx);
            }
            DefDiagnosticKind::InvalidDeriveTarget { ast, id } => {
                cx.tag("invalid derive target");
                ast.stable_hash(cx);
                id.stable_hash(cx);
            }
            DefDiagnosticKind::MalformedDerive { ast, id } => {
                cx.tag("malformed derive");
                ast.stable_hash(cx);
                id.stable_hash(cx);
            }
            DefDiagnosticKind::MacroDefError { ast, message: _message } => {
                cx.tag("macro definition error");
                ast.stable_hash(cx);
            }
            DefDiagnosticKind::MacroError { ast, path: _path, err: _err } => {
                cx.tag("macro error");
                ast.stable_hash(cx);
            }
        }
    }
}

impl StableHash for DefMap {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self {
            krate,
            block,
            root,
            modules,
            prelude,
            macro_use_prelude,
            derive_helpers_in_scope,
            macro_def_to_macro_id,
            diagnostics,
            data,
        } = self;
        cx.tag("def map");
        krate.stable_hash(cx);
        block.stable_hash(cx);
        root.stable_hash(cx);
        modules.stable_hash(cx);
        prelude.stable_hash(cx);
        cx.unordered(macro_use_prelude.iter(), |cx, (name, (macro_id, extern_crate))| {
            name.stable_hash(cx);
            macro_id.stable_hash(cx);
            extern_crate.stable_hash(cx);
        });
        cx.unordered(derive_helpers_in_scope.iter(), |cx, (item, helpers)| {
            item.stable_hash(cx);
            cx.write_u64(helpers.len() as u64);
            for (name, macro_id, call) in helpers {
                name.stable_hash(cx);
                macro_id.stable_hash(cx);
                call.stable_hash(cx);
            }
        });
        cx.unordered(macro_def_to_macro_id.iter(), |cx, (definition, macro_id)| {
            definition.stable_hash(cx);
            macro_id.stable_hash(cx);
        });
        diagnostics.stable_hash(cx);
        data.stable_hash(cx);
    }
}

impl StableHash for LocalDefMap {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { extern_prelude } = self;
        cx.tag("local def map");
        cx.unordered(extern_prelude.iter(), |cx, (name, (module, extern_crate))| {
            name.stable_hash(cx);
            module.stable_hash(cx);
            extern_crate.stable_hash(cx);
        });
    }
}

/// A hash of the definition map of `krate` (and of its extern prelude) that means the same in
/// every process, and how much of it could not be given a meaning that does.
pub fn def_map_fingerprint(db: &dyn base_db::SourceDatabase, krate: Crate) -> StableFingerprint {
    let pair = super::crate_local_def_map(db, krate);
    let mut cx = StableCx::new(db);
    pair.def_map(db).stable_hash(&mut cx);
    pair.local(db).stable_hash(&mut cx);
    cx.finish()
}
