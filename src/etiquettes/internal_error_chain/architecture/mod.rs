//! Rigid error architecture: library types that implement `Error` connect as
//! parent boxes Kind, Kind variants are native sources. Bin-only modules
//! (reachable only from `main.rs`) are not in the parent/Kind catalog; CLI
//! layout lints cover clap types, `act`, thin `main`, and bin-only `Error`
//! types instead.

use std::path::Path;

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::type_graph::for_each_src_rust_file;
use super::types::InternalErrorComplianceFinding;

use tracing::instrument;

mod catalog;
mod lint;

use catalog::{Catalog, CatalogPhase, CatalogVisitor};

/// Scan `src/**/*.rs` for the parent / Kind / native-source suite.
///
/// Membership is `impl Error` / `#[derive(Error)]`, not the `src/error` path.
/// Native sources may live next to the call site that produces them.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_architecture(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let mut catalog = Catalog::new(crate_name);
    for_each_src_rust_file(crate_root, |path, src_root| {
        load_src_file(&mut catalog, path, src_root)
    })?;
    catalog.into_findings()
}

#[instrument(level = "info", skip(catalog, file), err(level = "warn"))]
fn load_src_file(catalog: &mut Catalog, file: &Path, src_root: &Path) -> CordialResult<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = CatalogVisitor::new(
        file.to_path_buf(),
        module_prefix,
        catalog,
        CatalogPhase::Types,
    );
    visitor.visit_file(&syntax);
    visitor.set_phase(CatalogPhase::Impls);
    visitor.reset_module_prefix(module_path_from_src_file(src_root, file));
    visitor.visit_file(&syntax);
    Ok(())
}
