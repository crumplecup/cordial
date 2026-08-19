//! Clap / Error type catalog collected from library and binary files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ImplItem, ItemFn, ItemImpl};

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;

use super::super::tree::{item_derives_error, last_ident, trait_is_std_error, type_label};
use super::idents::{
    has_self_receiver, input_type_idents, item_derives, named_field_map, sig_returns_result,
    variant_shape,
};
use tracing::instrument;

pub(crate) struct TypeRec {
    pub(crate) ident: String,
    pub(crate) type_path: String,
    pub(crate) file: PathBuf,
    pub(crate) line: u32,
    pub(crate) snippet: String,
    pub(crate) parser: bool,
    pub(crate) subcommand: bool,
    pub(crate) error: bool,
    pub(crate) in_library: bool,
    pub(crate) fields: BTreeMap<String, Vec<String>>,
    pub(crate) variants: BTreeMap<String, VariantShape>,
}

pub(crate) enum VariantShape {
    Named(BTreeMap<String, Vec<String>>),
    Unnamed(Vec<Vec<String>>),
    Unit,
}

pub(crate) struct ActRec {
    pub(crate) file: PathBuf,
    pub(crate) line: u32,
    pub(crate) called_on: BTreeSet<String>,
}

pub(crate) struct PendingAct {
    pub(crate) ident: String,
    pub(crate) file: PathBuf,
    pub(crate) line: u32,
    pub(crate) block: syn::Block,
}

pub(crate) struct FreeFnRec {
    pub(crate) name: String,
    pub(crate) file: PathBuf,
    pub(crate) line: u32,
    pub(crate) in_library: bool,
    pub(crate) input_idents: Vec<String>,
}

pub(crate) struct LayoutCatalog {
    pub(crate) crate_name: String,
    pub(crate) types: BTreeMap<String, TypeRec>,
    pub(crate) acts: BTreeMap<String, ActRec>,
    pub(crate) pending_acts: Vec<PendingAct>,
    pub(crate) free_fns: Vec<FreeFnRec>,
}

#[instrument(level = "info", skip(catalog, file), err(level = "warn"))]
pub(crate) fn load_file(
    catalog: &mut LayoutCatalog,
    file: &Path,
    in_library: bool,
) -> CordialResult<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let mut visitor = LayoutVisitor {
        file: file.to_path_buf(),
        in_library,
        catalog,
        error_impls: BTreeSet::new(),
    };
    visitor.visit_file(&syntax);
    for ident in visitor.error_impls {
        if let Some(item) = catalog.types.get_mut(&ident) {
            item.error = true;
        }
    }
    Ok(())
}

struct LayoutVisitor<'a> {
    file: PathBuf,
    in_library: bool,
    catalog: &'a mut LayoutCatalog,
    error_impls: BTreeSet<String>,
}

struct TypeSeed {
    ident: String,
    line: u32,
    snippet: String,
    parser: bool,
    subcommand: bool,
    error: bool,
    fields: BTreeMap<String, Vec<String>>,
    variants: BTreeMap<String, VariantShape>,
}

impl LayoutVisitor<'_> {
    #[instrument(level = "debug", skip(self, seed))]
    fn upsert_type(&mut self, seed: TypeSeed) {
        let entry = self
            .catalog
            .types
            .entry(seed.ident.clone())
            .or_insert(TypeRec {
                ident: seed.ident.clone(),
                type_path: format!("{}::{}", self.catalog.crate_name, seed.ident),
                file: self.file.clone(),
                line: seed.line,
                snippet: seed.snippet.clone(),
                parser: false,
                subcommand: false,
                error: false,
                in_library: self.in_library,
                fields: BTreeMap::new(),
                variants: BTreeMap::new(),
            });
        entry.parser |= seed.parser;
        entry.subcommand |= seed.subcommand;
        entry.error |= seed.error;
        entry.fields.extend(seed.fields);
        entry.variants.extend(seed.variants);
        if self.in_library {
            entry.in_library = true;
        }
    }
}

impl<'ast> Visit<'ast> for LayoutVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let ident = node.ident.to_string();
        self.upsert_type(TypeSeed {
            ident,
            line: node.span().start().line as u32,
            snippet: format!("struct {}", node.ident),
            parser: item_derives(&node.attrs, "Parser"),
            subcommand: item_derives(&node.attrs, "Subcommand"),
            error: item_derives_error(&node.attrs),
            fields: named_field_map(&node.fields),
            variants: BTreeMap::new(),
        });
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let ident = node.ident.to_string();
        let mut variants = BTreeMap::new();
        for variant in &node.variants {
            variants.insert(variant.ident.to_string(), variant_shape(&variant.fields));
        }
        self.upsert_type(TypeSeed {
            ident,
            line: node.span().start().line as u32,
            snippet: format!("enum {}", node.ident),
            parser: item_derives(&node.attrs, "Parser"),
            subcommand: item_derives(&node.attrs, "Subcommand"),
            error: item_derives_error(&node.attrs),
            fields: BTreeMap::new(),
            variants,
        });
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.catalog.free_fns.push(FreeFnRec {
            name: node.sig.ident.to_string(),
            file: self.file.clone(),
            line: node.span().start().line as u32,
            in_library: self.in_library,
            input_idents: input_type_idents(&node.sig),
        });
        syn::visit::visit_item_fn(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let self_ident = last_ident(&type_label(&node.self_ty)).to_string();
        if let Some((_, trait_path, _)) = &node.trait_
            && trait_is_std_error(trait_path)
        {
            self.error_impls.insert(self_ident);
            return;
        }
        if node.trait_.is_some() {
            return;
        }
        for impl_item in &node.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            if method.sig.ident != "act" {
                continue;
            }
            if !has_self_receiver(&method.sig) || !sig_returns_result(&method.sig) {
                continue;
            }
            self.catalog.pending_acts.push(PendingAct {
                ident: self_ident.clone(),
                file: self.file.clone(),
                line: method.span().start().line as u32,
                block: method.block.clone(),
            });
        }
    }
}
