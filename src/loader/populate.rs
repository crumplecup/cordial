use std::path::Path;

use syn::spanned::Spanned;

use crate::error::CordialResult;
use crate::ir::{CrateIr, EdgeKind, ItemKind, NodeKind, NodeWeight};
use crate::objects::FileSpan;

use super::module_path_from_src_file;
use super::source::{SourceFile, SourceLoadView, SourceLoader};

impl SourceLoadView {
    pub fn populate_ir(&self, ir: &mut CrateIr) -> CordialResult<()> {
        let root = ir.root;
        for file in &self.files {
            self.load_file(ir, root, file)?;
        }
        Ok(())
    }

    fn load_file(
        &self,
        ir: &mut CrateIr,
        parent: crate::ir::NodeId,
        file: &SourceFile,
    ) -> CordialResult<()> {
        let syntax = syn::parse_file(&file.source).map_err(|err| {
            crate::error::CordialError::syn_parse(file.path.display().to_string(), err)
        })?;

        let parts = module_path_from_src_file(&self.src_root, &file.path);
        let module_path = parts.join("::");
        let module_node = ir.insert_node(
            NodeWeight::new(NodeKind::Module)
                .with_name(module_path.clone())
                .with_span(line_span(&file.path, 1)),
        );
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

        for item in syntax.items {
            self.load_item(ir, module_node, &module_path, &file.path, item)?;
        }
        Ok(())
    }

    fn load_item(
        &self,
        ir: &mut CrateIr,
        parent: crate::ir::NodeId,
        module_path: &str,
        file: &Path,
        item: syn::Item,
    ) -> CordialResult<()> {
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
        Ok(())
    }
}

fn line_span(file: &Path, line: u32) -> FileSpan {
    FileSpan::new(file, line, 1)
}
