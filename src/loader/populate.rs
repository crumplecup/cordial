use std::path::Path;

use syn::spanned::Spanned;

use crate::error::CordialResult;
use crate::ir::{ATTR_CRATE_KINDS, CrateIr, EdgeKind, ItemKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::module_path_from_src_file;
use super::source::{SourceFile, SourceLoadView, SourceLoader};

use tracing::instrument;
impl SourceLoadView {
    /// Load the same source view used by the session for a crate target.
    #[instrument(level = "debug", skip(target), err(level = "warn"))]
    pub(crate) fn for_target(target: &super::CrateTarget) -> CordialResult<Self> {
        let src_root = target.crate_root().join("src");
        let view = Self::new(
            target.crate_name().clone(),
            src_root.clone(),
            read_source_files(&src_root)?,
            target.kinds().clone(),
        )
        .with_binary_entries(target.binary_entries().clone())
        .with_manifest_snapshot(target.manifest_snapshot().clone());
        Ok(view)
    }

    /// Load a standalone crate's source through the same snapshot path.
    #[cfg(feature = "crate_attrs")]
    #[instrument(level = "debug", skip(crate_root), err(level = "warn"))]
    pub(crate) fn for_crate_root(crate_root: &Path, crate_name: &str) -> CordialResult<Self> {
        let src_root = crate_root.join("src");
        Ok(Self::new(
            crate_name.to_string(),
            src_root.clone(),
            read_source_files(&src_root)?,
            Vec::new(),
        ))
    }
}

#[instrument(level = "debug", skip(src_root), err(level = "warn"))]
fn read_source_files(src_root: &Path) -> CordialResult<Vec<SourceFile>> {
    let mut files = Vec::new();
    if src_root.is_dir() {
        for entry in walkdir::WalkDir::new(src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let source = super::source_work::read(path, SourceLoader::ID)?;
            files.push(SourceFile::new(path.to_path_buf(), source));
        }
    }
    Ok(files)
}

impl SourceLoadView {
    /// Fill the crate IR from loaded views.
    #[instrument(level = "debug", skip(self, ir), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    pub fn populate_ir(&self, ir: &mut CrateIr) -> CordialResult<()> {
        let root = ir.root();
        if let Some(snapshot) = self.manifest_snapshot() {
            ir.set_attr(
                root,
                crate::ir::ATTR_CARGO_MANIFEST_SNAPSHOT,
                serde_json::to_value(snapshot)?,
            )?;
        }
        ir.set_attr(
            root,
            crate::ir::ATTR_BINARY_ENTRY_PATHS,
            serde_json::to_value(self.binary_entries())?,
        )?;
        if !self.crate_kinds().is_empty() {
            ir.set_attr(
                root,
                ATTR_CRATE_KINDS,
                serde_json::Value::Array(
                    self.crate_kinds()
                        .iter()
                        .map(|kind| serde_json::Value::String(kind.as_str().to_string()))
                        .collect(),
                ),
            )?;
        }
        #[cfg(any(
            feature = "allows",
            feature = "cfg_hygiene",
            feature = "cfg_scatter",
            feature = "glob_imports",
            feature = "inline_tests",
            feature = "modularity",
            feature = "pageantry",
            feature = "derives",
            feature = "visibility"
        ))]
        if let Some(crate_root) = self.src_root().parent() {
            let inventory = crate::ir::SourcePolicyInventory::collect(self, crate_root)?;
            ir.set_attr(
                root,
                crate::ir::ATTR_SOURCE_POLICY_INVENTORY,
                serde_json::to_value(inventory)?,
            )?;
        }
        for file in self.files() {
            let syntax = self.parsed_file(file.path(), "source-ir")?;
            self.load_file(ir, root, &syntax)?;
        }
        if let Some(crate_root) = self.src_root().parent()
            && let Some(lib) = super::library_root_rs(crate_root)
            && lib.is_file()
            && !self.files().iter().any(|file| file.path() == &lib)
        {
            let syntax = self.parsed_file(&lib, "source-ir")?;
            self.load_file(ir, root, &syntax)?;
        }
        Ok(())
    }

    #[instrument(level = "info", skip(self, ir, parent, file), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    fn load_file(
        &self,
        ir: &mut CrateIr,
        parent: crate::ir::NodeId,
        file: &super::SharedParsedFile,
    ) -> CordialResult<()> {
        let syntax = file;

        let parts = module_path_from_src_file(self.src_root(), file.path());
        let module_path = parts.join("::");
        let module_node = ir.insert_node(
            NodeWeight::new(NodeKind::Module)
                .with_name(module_path.clone())
                .with_span(line_span(file.path(), 1)),
        );
        ir.set_attr(
            module_node,
            crate::ir::ATTR_SOURCE_MODULE_SCOPE,
            serde_json::to_value(crate::ir::SourceModuleScope::collect(syntax))?,
        )?;
        ir.set_attr(
            module_node,
            crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES,
            serde_json::to_value(crate::ir::SourceFileAttributes::collect(syntax))?,
        )?;
        #[cfg(feature = "tracing")]
        ir.set_attr(
            module_node,
            crate::ir::ATTR_SOURCE_TRACING_INVENTORY,
            serde_json::to_value(crate::ir::SourceTracingInventory::collect(syntax)?)?,
        )?;
        ir.set_attr(
            module_node,
            "qualified_path",
            serde_json::Value::String(module_path.clone()),
        )?;
        ir.set_attr(
            module_node,
            SourceLoader::ATTR_IR_ORIGIN,
            serde_json::Value::String(SourceLoader::ORIGIN.to_string()),
        )?;
        ir.insert_edge(parent, module_node, EdgeKind::Contains)?;

        super::source_work::walk(file.path(), "source-ir", || {
            for (index, item) in syntax.syntax().items.iter().enumerate() {
                let node = self.load_item(ir, module_node, &module_path, file.path(), item)?;
                if let Some(facts) = crate::ir::FunctionSourceSyntax::collect(syntax, index) {
                    ir.set_attr(
                        node,
                        crate::ir::ATTR_FUNCTION_SOURCE_SYNTAX,
                        serde_json::to_value(facts)?,
                    )?;
                }
            }
            Ok::<(), crate::error::CordialError>(())
        })?;
        Ok(())
    }

    #[instrument(
        level = "info",
        skip(self, ir, parent, file, item),
        err(level = "warn")
    )]
    fn load_item(
        &self,
        ir: &mut CrateIr,
        parent: crate::ir::NodeId,
        module_path: &str,
        file: &Path,
        item: &syn::Item,
    ) -> CordialResult<crate::ir::NodeId> {
        let (name, kind) = match &item {
            syn::Item::Fn(item_fn) => (item_fn.sig.ident.to_string(), ItemKind::Fn),
            syn::Item::Struct(item_struct) => (item_struct.ident.to_string(), ItemKind::Struct),
            syn::Item::Enum(item_enum) => (item_enum.ident.to_string(), ItemKind::Enum),
            syn::Item::Trait(item_trait) => (item_trait.ident.to_string(), ItemKind::Trait),
            syn::Item::Type(item_type) => (item_type.ident.to_string(), ItemKind::TypeAlias),
            syn::Item::Const(item_const) => (item_const.ident.to_string(), ItemKind::Const),
            syn::Item::Static(item_static) => (item_static.ident.to_string(), ItemKind::Static),
            syn::Item::Macro(item_macro) => (
                item_macro
                    .ident
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "macro".to_string()),
                ItemKind::Macro,
            ),
            syn::Item::Mod(item_mod) => (item_mod.ident.to_string(), ItemKind::Mod),
            _ => ("item".to_string(), ItemKind::Other),
        };

        let line = item.span().start().line as u32;
        let item_path = if module_path.is_empty() {
            name.clone()
        } else {
            format!("{module_path}::{name}")
        };

        let item_node = ir.insert_node(
            NodeWeight::new(NodeKind::Item(kind))
                .with_name(name)
                .with_span(FileSpan::new(file, line, 1)),
        );
        ir.set_attr(
            item_node,
            "qualified_path",
            serde_json::Value::String(item_path),
        )?;
        ir.set_attr(
            item_node,
            SourceLoader::ATTR_IR_ORIGIN,
            serde_json::Value::String(SourceLoader::ORIGIN.to_string()),
        )?;
        ir.insert_edge(parent, item_node, EdgeKind::Contains)?;
        Ok(item_node)
    }
}

#[instrument(level = "debug", skip(file))]
fn line_span(file: &Path, line: u32) -> FileSpan {
    FileSpan::new(file, line, 1)
}
