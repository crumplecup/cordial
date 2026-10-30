//! One syntax walk per source file, dispatching sites to candidate fields.

mod helpers;
mod visit;

use syn::visit::Visit;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::try_conversion::ErrorTryEvidence;
use super::{ConversionCandidate, ErrorConversionSite};

pub(super) struct BatchInventoryVisitor<'a> {
    file: &'a SharedParsedFile,
    candidates: &'a [ConversionCandidate],
    relevant: &'a [usize],
    declarations: &'a super::CrateTypeIndex,
    module_depth: usize,
    current_impl: Option<String>,
    current_error_trait: Option<syn::Path>,
    current_source_sites: Vec<(usize, usize)>,
    returning_error: Vec<bool>,
    try_evidence: Option<ErrorTryEvidence>,
    sites: Vec<Vec<ErrorConversionSite>>,
}

impl<'a> BatchInventoryVisitor<'a> {
    #[instrument(level = "debug", skip(file, candidates, relevant, declarations))]
    pub(super) fn collect(
        file: &'a SharedParsedFile,
        candidates: &'a [ConversionCandidate],
        relevant: &'a [usize],
        declarations: &'a super::CrateTypeIndex,
    ) -> Vec<Vec<ErrorConversionSite>> {
        let mut visitor = Self {
            file,
            candidates,
            relevant,
            declarations,
            module_depth: 0,
            current_impl: None,
            current_error_trait: None,
            current_source_sites: Vec::new(),
            returning_error: vec![false; candidates.len()],
            try_evidence: None,
            sites: vec![Vec::new(); candidates.len()],
        };
        visitor.visit_file(file.syntax());
        visitor.sites
    }
}
