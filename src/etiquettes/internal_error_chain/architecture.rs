//! Rigid error architecture: library types that implement `Error` connect as
//! parent boxes Kind, Kind variants are native sources. Bin-only modules
//! (reachable only from `main.rs`) are not in the parent/Kind catalog; CLI
//! layout lints cover clap types, `act`, thin `main`, and bin-only `Error`
//! types instead.

use std::path::Path;

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::type_graph::{for_each_shared_src_rust_file, for_each_src_rust_file};
use super::types::InternalErrorComplianceFinding;

use tracing::instrument;

mod catalog;
mod lint;

use catalog::{Catalog, CatalogVisitor};

/// Scan `src/**/*.rs` for the parent / Kind / native-source suite.
///
/// Membership is `impl Error` / `#[derive(Error)]`, not the `src/error` path.
/// Native sources may live next to the call site that produces them.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_architecture(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    scan_crate_error_architecture_impl(
        crate_root,
        crate_name,
        None,
        crate::config::InternalErrorChainThresholds::default().kind_fanout_max_call_sites(),
    )
}

#[instrument(level = "debug", skip(source), err(level = "warn"))]
pub(crate) fn scan_crate_error_architecture_with_source(
    crate_root: &Path,
    crate_name: &str,
    source: &crate::loader::SourceLoadView,
    kind_fanout_max_call_sites: usize,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    scan_crate_error_architecture_impl(
        crate_root,
        crate_name,
        Some(source),
        kind_fanout_max_call_sites,
    )
}

#[instrument(level = "debug", skip(source), err(level = "warn"))]
#[hotpath::measure]
fn scan_crate_error_architecture_impl(
    crate_root: &Path,
    crate_name: &str,
    source: Option<&crate::loader::SourceLoadView>,
    kind_fanout_max_call_sites: usize,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let mut catalog = Catalog::new(crate_name);
    if let Some(source) = source {
        for_each_shared_src_rust_file(source, crate_root, |path, src_root| {
            load_src_file(&mut catalog, path, src_root, Some(source))
        })?;
    } else {
        for_each_src_rust_file(crate_root, |path, src_root| {
            load_src_file(&mut catalog, path, src_root, None)
        })?;
    }
    catalog.into_findings(kind_fanout_max_call_sites)
}

#[instrument(level = "info", skip(catalog, file, source), err(level = "warn"))]
fn load_src_file(
    catalog: &mut Catalog,
    file: &Path,
    src_root: &Path,
    source: Option<&crate::loader::SourceLoadView>,
) -> CordialResult<()> {
    enum ParsedInput {
        Shared(crate::loader::SharedParsedFile),
        Standalone(syn::File),
    }
    let input = if let Some(source) = source {
        ParsedInput::Shared(source.parsed_file(file, "error-architecture")?)
    } else {
        let text = crate::loader::source_read(file, "error-architecture")?;
        ParsedInput::Standalone(crate::loader::source_parse(
            &text,
            file,
            "error-architecture",
        )?)
    };
    let syntax = match &input {
        ParsedInput::Shared(file) => file.syntax(),
        ParsedInput::Standalone(syntax) => syntax,
    };
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = CatalogVisitor::builder()
        .file(file.to_path_buf())
        .module_prefix(module_prefix)
        .catalog(catalog)
        .build()?;
    crate::loader::source_walk(file, "error-architecture", || visitor.visit_file(syntax));
    Ok(())
}
