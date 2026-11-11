//! Resolve the library root and inspect crate-level inner attributes.

pub use crate::loader::library_root_rs;
use std::path::Path;

use crate::ir::{IrView, QueryBuilder, SourceCrateLints, SourceFileAttributes};

use crate::config::CrateAttrsThresholds;
use crate::error::CordialResult;
use crate::objects::SourceSpan;

use super::types::{CrateAttrsRuleId, CrateAttrsSiteRecord};

use tracing::instrument;

/// Scan the crate's library root for missing crate-level lint attributes.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_attrs(
    crate_root: &Path,
    crate_name: &str,
    policy: &CrateAttrsThresholds,
) -> CordialResult<Vec<CrateAttrsSiteRecord>> {
    let Some(lib) = library_root_rs(crate_root) else {
        return Ok(Vec::new());
    };

    let parsed = crate::loader::ParsedSourceSession::begin_in(
        crate_root,
        &crate::loader::Reporter::default(),
        crate::CfgPlanning::Explicit,
    )?;
    let target = crate::discover_crate_targets(crate_root, &crate::RunAll)?
        .into_iter()
        .find(|target| target.crate_name() == crate_name)
        .ok_or_else(|| crate::CordialError::not_found(crate_root.to_path_buf()))?;
    let ir = parsed.project_target(&target)?;
    let presence = library_facts(&ir, &lib)?
        .map(|facts| *facts.lints())
        .unwrap_or_default();
    let verus_exempt = verus_exemption::is_unsafe_forbid_exempt_verus_target(crate_root, &ir)?;
    records_for_presence(crate_root, crate_name, policy, &lib, presence, verus_exempt)
}

#[instrument(level = "debug", skip(ir, policy), err(level = "warn"))]
pub(super) fn scan_shared_crate_attrs(
    ir: &dyn IrView,
    crate_root: &Path,
    crate_name: &str,
    policy: &CrateAttrsThresholds,
) -> CordialResult<(Vec<CrateAttrsSiteRecord>, bool)> {
    let Some(lib) = library_root_rs(crate_root) else {
        return Ok((Vec::new(), false));
    };
    let presence = library_facts(ir, &lib)?
        .map(|facts| *facts.lints())
        .unwrap_or_default();
    let verus_exempt = verus_exemption::is_unsafe_forbid_exempt_verus_target(crate_root, ir)?;
    let records =
        records_for_presence(crate_root, crate_name, policy, &lib, presence, verus_exempt)?;
    Ok((records, verus_exempt))
}

#[instrument(level = "debug", skip(policy, presence), err(level = "warn"))]
fn records_for_presence(
    crate_root: &Path,
    crate_name: &str,
    policy: &CrateAttrsThresholds,
    lib: &Path,
    presence: SourceCrateLints,
    verus_exempt: bool,
) -> CordialResult<Vec<CrateAttrsSiteRecord>> {
    let mut file = lib.to_path_buf();
    if let Ok(rel) = file.strip_prefix(crate_root) {
        file = rel.to_path_buf();
    }
    let line = 1;
    let mut records = Vec::new();
    if !policy.skip_unsafe(crate_name) && !presence.forbid_unsafe_code() && !verus_exempt {
        records.push(
            CrateAttrsSiteRecord::builder()
                .rule_id(CrateAttrsRuleId::ForbidUnsafe001)
                .context(crate_name.to_string())
                .file(file.clone())
                .line(line)
                .snippet("add `#![forbid(unsafe_code)]`".to_string())
                .build()?,
        );
    }
    if !policy.skip_missing_docs(crate_name) && !presence.missing_docs_at_least_warn() {
        records.push(
            CrateAttrsSiteRecord::builder()
                .rule_id(CrateAttrsRuleId::MissingDocs001)
                .context(crate_name.to_string())
                .file(file)
                .line(line)
                .snippet("add `#![warn(missing_docs)]`".to_string())
                .build()?,
        );
    }
    Ok(records)
}

/// A verus-target crate that states real `pub assume_specification
/// [..] (..) -> (..) ..;` claims about external functions is exempt from
/// `CRATE-FORBID-UNSAFE-001`: the real `verus` binary (invoked as a bare
/// compiler, never through `cargo`) expands `assume_specification` into
/// constructs its own `forbid(unsafe_code)` genuinely rejects
/// ("declaration of an unsafe function" / "usage of an unsafe block"),
/// even though the crate's own source never writes `unsafe` by hand --
/// confirmed empirically against a real `verus --crate-type=lib`
/// invocation, not assumed. Plain `cargo build`/clippy never see this;
/// only the real Verus toolchain does. Both signals required: a
/// verus-target crate that never actually states an external claim this
/// way has no reason to be exempt.
#[cfg(feature = "verus_warnings")]
mod verus_exemption {
    use crate::CordialResult;
    use crate::ir::{ATTR_SOURCE_FILE_ATTRIBUTES, IrView, QueryBuilder};
    use std::path::Path;

    #[tracing::instrument(level = "trace", skip(ir), err(level = "warn"))]
    pub(super) fn is_unsafe_forbid_exempt_verus_target(
        crate_root: &Path,
        ir: &dyn IrView,
    ) -> CordialResult<bool> {
        if !crate::etiquettes::verus_warnings::crate_is_verus_target(crate_root) {
            return Ok(false);
        }
        for node in ir.nodes_matching(
            &QueryBuilder::new()
                .has_attr(ATTR_SOURCE_FILE_ATTRIBUTES)
                .build(),
        ) {
            if ir
                .source_file_attributes(node.id())?
                .is_some_and(|facts| facts.assume_specification())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(not(feature = "verus_warnings"))]
mod verus_exemption {
    use crate::{CordialResult, ir::IrView};
    use std::path::Path;

    #[tracing::instrument(level = "trace", skip(ir))]
    pub(super) fn is_unsafe_forbid_exempt_verus_target(
        crate_root: &Path,
        ir: &dyn IrView,
    ) -> CordialResult<bool> {
        let _ = (crate_root, ir);
        Ok(false)
    }
}

/// Read the library's collected facts; an existing file without evidence is an error.
#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(super) fn library_facts(
    ir: &dyn IrView,
    lib: &Path,
) -> CordialResult<Option<SourceFileAttributes>> {
    for node in ir.nodes_matching(
        &QueryBuilder::new()
            .has_attr(crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES)
            .build(),
    ) {
        if node
            .weight()
            .span()
            .as_ref()
            .is_some_and(|span| span.file() == lib)
        {
            return ir.source_file_attributes(node.id());
        }
    }
    if lib.is_file() {
        return Err(crate::CordialError::workspace_source_facts_missing(
            "library attributes",
        ));
    }
    Ok(None)
}
