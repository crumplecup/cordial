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

#[derive(derive_getters::Getters, derive_new::new)]
pub(crate) struct TypeRec {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    #[getter(copy)]
    parser: bool,
    #[getter(copy)]
    subcommand: bool,
    #[getter(copy)]
    error: bool,
    #[getter(copy)]
    in_library: bool,
    fields: BTreeMap<String, Vec<String>>,
    variants: BTreeMap<String, VariantShape>,
}

pub(crate) enum VariantShape {
    Named(BTreeMap<String, Vec<String>>),
    Unnamed(Vec<Vec<String>>),
    Unit,
}

#[derive(derive_getters::Getters, derive_new::new)]
pub(crate) struct ActRec {
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    called_on: BTreeSet<String>,
}

#[derive(derive_getters::Getters, derive_new::new)]
pub(crate) struct PendingAct {
    ident: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    block: syn::Block,
}

impl PendingAct {
    #[instrument(level = "debug", skip(self))]
    pub(crate) fn into_parts(self) -> (String, PathBuf, u32, syn::Block) {
        (self.ident, self.file, self.line, self.block)
    }
}

#[derive(derive_getters::Getters, derive_new::new)]
pub(crate) struct FreeFnRec {
    name: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    in_library: bool,
    input_idents: Vec<String>,
}

#[derive(derive_getters::Getters)]
pub(crate) struct LayoutCatalog {
    crate_name: String,
    types: BTreeMap<String, TypeRec>,
    acts: BTreeMap<String, ActRec>,
    pending_acts: Vec<PendingAct>,
    free_fns: Vec<FreeFnRec>,
}

impl LayoutCatalog {
    #[instrument(level = "debug", fields(crate_name = crate_name))]
    pub(crate) fn new(crate_name: String) -> Self {
        Self {
            crate_name,
            types: BTreeMap::new(),
            acts: BTreeMap::new(),
            pending_acts: Vec::new(),
            free_fns: Vec::new(),
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(crate) fn acts_mut(&mut self) -> &mut BTreeMap<String, ActRec> {
        &mut self.acts
    }

    #[instrument(level = "debug", skip(self))]
    pub(crate) fn pending_acts_mut(&mut self) -> &mut Vec<PendingAct> {
        &mut self.pending_acts
    }
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
            .or_insert_with(|| {
                TypeRec::new(
                    seed.ident.clone(),
                    format!("{}::{}", self.catalog.crate_name, seed.ident),
                    self.file.clone(),
                    seed.line,
                    seed.snippet.clone(),
                    false,
                    false,
                    false,
                    self.in_library,
                    BTreeMap::new(),
                    BTreeMap::new(),
                )
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
        self.catalog.free_fns.push(FreeFnRec::new(
            node.sig.ident.to_string(),
            self.file.clone(),
            node.span().start().line as u32,
            self.in_library,
            input_type_idents(&node.sig),
        ));
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
            self.catalog.pending_acts.push(PendingAct::new(
                self_ident.clone(),
                self.file.clone(),
                method.span().start().line as u32,
                method.block.clone(),
            ));
        }
    }
}
