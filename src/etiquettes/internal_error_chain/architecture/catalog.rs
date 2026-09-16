//! Collected Error-implementing types for architecture lints.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::super::type_graph::last_ident;

use tracing::instrument;

mod visitor;

pub(super) use visitor::{CatalogPhase, CatalogVisitor};

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct StructInfo {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    kind_box_of: Option<String>,
    kind_unboxed_of: Option<String>,
    foreign_source: Option<String>,
    #[getter(copy)]
    has_source_field: bool,
    #[getter(copy)]
    has_file: bool,
    #[getter(copy)]
    has_line: bool,
    #[getter(copy)]
    has_location: bool,
}

impl StructInfo {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn location_complete(&self) -> bool {
        self.has_file && self.has_line
    }
}

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct VariantInfo {
    name: String,
    #[getter(copy)]
    line: u32,
    snippet: String,
    payloads: Vec<String>,
}

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct EnumInfo {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    variants: Vec<VariantInfo>,
}

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct ConstructorRec {
    self_ident: String,
    name: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    has_track_caller: bool,
    #[getter(copy)]
    captures_location: bool,
    #[getter(copy)]
    from_trait: bool,
    input_labels: Vec<String>,
    #[getter(copy)]
    takes_location_arg: bool,
}

#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct Catalog {
    crate_name: String,
    structs: BTreeMap<String, StructInfo>,
    enums: BTreeMap<String, EnumInfo>,
    constructors: Vec<ConstructorRec>,
    error_impls: BTreeSet<String>,
}

impl Catalog {
    #[instrument(level = "debug", fields(crate_name = crate_name))]
    pub(super) fn new(crate_name: &str) -> Self {
        Self {
            crate_name: crate_name.to_string(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            constructors: Vec::new(),
            error_impls: BTreeSet::new(),
        }
    }

    #[instrument(level = "debug")]
    pub(super) fn last_ident(label: &str) -> &str {
        last_ident(label)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn impls_error(&self, ident: &str) -> bool {
        self.error_impls.contains(ident)
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_kind_name(ident: &str) -> bool {
        ident.ends_with("Kind")
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_error_enum_name(ident: &str) -> bool {
        ident.ends_with("Error") && !Self::is_kind_name(ident)
    }

    #[instrument(level = "trace", skip(self, item))]
    pub(super) fn is_error_kind(&self, item: &EnumInfo) -> bool {
        let boxed_by_error = self.structs.values().any(|item_struct| {
            self.impls_error(&item_struct.ident)
                && (item_struct.kind_box_of.as_deref() == Some(item.ident.as_str())
                    || item_struct.kind_unboxed_of.as_deref() == Some(item.ident.as_str()))
        });
        if boxed_by_error {
            return true;
        }
        Self::is_kind_name(&item.ident)
            && item.variants.iter().any(|variant| {
                variant
                    .payloads
                    .iter()
                    .any(|payload| self.impls_error(Self::last_ident(payload)))
            })
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn kind_payload_idents(&self) -> BTreeSet<String> {
        let mut idents = BTreeSet::new();
        for item in self.enums.values() {
            if !self.is_error_kind(item) {
                continue;
            }
            for variant in &item.variants {
                for payload in &variant.payloads {
                    idents.insert(Self::last_ident(payload).to_string());
                }
            }
        }
        idents
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn root_parents(&self) -> Vec<&StructInfo> {
        let payloads = self.kind_payload_idents();
        self.structs
            .values()
            .filter(|item| {
                self.impls_error(&item.ident)
                    && item.kind_box_of.is_some()
                    && !payloads.contains(&item.ident)
            })
            .collect()
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn native_source_idents(&self) -> BTreeSet<String> {
        let parents: BTreeSet<String> = self
            .root_parents()
            .into_iter()
            .map(|item| item.ident.clone())
            .collect();
        self.error_impls
            .iter()
            .filter(|ident| self.structs.contains_key(*ident) && !parents.contains(*ident))
            .cloned()
            .collect()
    }
}
