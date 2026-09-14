//! Assessor for amenable ext (jiff) registry coverage — the
//! `amenable_ext` counterpart of
//! `etiquettes::framework_std::assessor::AmenableStdAssessor`, which
//! does the identical job for `amenable_std`.

use crate::error::CordialResult;
use crate::framework_std::{
    AMENABLE_EXT_IMPL_CRATE, AMENABLE_EXT_JIFF_PATCH_SET, AMENABLE_EXT_JIFF_UPSTREAM_CRATE,
    AmenableExtOptions, ClassifyRowArgs, amenable_ext_gap_fields, classify_amenable_ext_row,
    collect_proof_chain_subjects, ensure_registry_dump_for_assessor,
    load_ext_inventory_from_shadow_dep, load_verifier_skip_map,
};
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Finding, NodeAnchor};
use crate::store::StoreLayout;

use super::jiff::{
    AmenableExtJiffRowFinding, AmenableExtJiffRule, amenable_ext_jiff_row_disposition,
};
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtJiffAssessor;

impl AmenableExtJiffAssessor {
    pub const ID: &'static str = "amenable-ext-jiff-assessor";
}

impl Assessor for AmenableExtJiffAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &[super::probe::AmenableExtJiffScopeProbe::ID]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers();
        let (ir, _, session) = view.into_parts();

        if markers.is_empty() || ir.crate_name() != AMENABLE_EXT_IMPL_CRATE {
            return Ok(Vec::new());
        }

        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let options = AmenableExtOptions::default();
        let items = load_ext_inventory_from_shadow_dep(
            session.project_root(),
            &store,
            AMENABLE_EXT_IMPL_CRATE,
            AMENABLE_EXT_JIFF_UPSTREAM_CRATE,
            options.force_rustdoc(),
        )?;
        let registry_options = crate::framework_std::AmenableStdOptions::default();
        let registry =
            ensure_registry_dump_for_assessor(&store, session.project_root(), &registry_options)?;
        let skip_map = load_verifier_skip_map(&store, AMENABLE_EXT_JIFF_PATCH_SET);
        let proof_chain_subjects = collect_proof_chain_subjects(session.project_root())?;
        let anchor = NodeAnchor::new(ir.root()?);

        let mut findings = Vec::new();
        for marker in markers {
            let Some(type_path) = marker.field("type_path") else {
                continue;
            };
            let item = items.iter().find(|item| item.path() == type_path);
            let entry = classify_amenable_ext_row(
                type_path,
                ClassifyRowArgs::new(
                    marker.field("type_kind").unwrap_or(""),
                    marker.field("is_generic") == Some("true"),
                    item.and_then(|item| item.alias_target().as_deref()),
                    &items,
                    &registry,
                    &skip_map,
                    &proof_chain_subjects,
                ),
            );
            let (missing_layers, action) = amenable_ext_gap_fields(&entry, AMENABLE_EXT_IMPL_CRATE);
            findings.push(Box::new(AmenableExtJiffRowFinding::new(
                AmenableExtJiffRule,
                amenable_ext_jiff_row_disposition(entry.status()),
                anchor,
                AMENABLE_EXT_JIFF_UPSTREAM_CRATE.to_string(),
                AMENABLE_EXT_IMPL_CRATE.to_string(),
                entry.type_path().clone(),
                entry.type_kind().clone(),
                entry.is_generic(),
                entry.status(),
                entry.evidence_link(),
                entry.evidence_name().clone(),
                entry.kani_witness(),
                entry.creusot_witness(),
                entry.verus_witness(),
                entry.proof_test(),
                entry.skip_reason().clone(),
                entry.kani_excepted(),
                entry.creusot_excepted(),
                entry.verus_excepted(),
                missing_layers,
                action,
            )) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
