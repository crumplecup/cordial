//! Assessor for amenable ext (third-party crate) registry coverage — one
//! generic implementation shared by every `amenable-ext-{target}`
//! etiquette instance, parameterized by the upstream crate name.

use crate::error::CordialResult;
use crate::framework_std::{
    AMENABLE_EXT_IMPL_CRATE, AmenableExtOptions, ClassifyRowArgs, amenable_ext_gap_fields,
    classify_amenable_ext_row, collect_proof_chain_subjects, ensure_registry_dump_for_assessor,
    load_ext_inventory_from_shadow_dep, load_verifier_skip_map,
};
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Finding, NodeAnchor};
use crate::store::StoreLayout;

use super::row::{ExtRowFinding, ExtRowRule, ext_row_disposition};
use tracing::instrument;

/// Assessor for one amenable-ext target crate's inventory rows.
#[derive(Debug, Clone)]
pub struct ExtAssessor {
    id: String,
    probe_id: String,
    upstream_crate: String,
    patch_set: String,
    rule: ExtRowRule,
}

impl ExtAssessor {
    /// Build the assessor for `target` (e.g. `"jiff"`).
    #[instrument(level = "debug")]
    pub fn new(target: &str) -> Self {
        Self {
            id: format!("amenable-ext-{target}-assessor"),
            probe_id: format!("amenable-ext-{target}-scope"),
            upstream_crate: target.to_string(),
            patch_set: format!("amenable_ext_{target}"),
            rule: ExtRowRule::new(target),
        }
    }
}

impl Assessor for ExtAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.id
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> Vec<&str> {
        vec![self.probe_id.as_str()]
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
            &self.upstream_crate,
            options.force_rustdoc(),
        )?;
        let registry_options = crate::framework_std::AmenableStdOptions::default();
        let registry =
            ensure_registry_dump_for_assessor(&store, session.project_root(), &registry_options)?;
        let skip_map = load_verifier_skip_map(&store, &self.patch_set);
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
            )?;
            let (missing_layers, action) = amenable_ext_gap_fields(&entry, AMENABLE_EXT_IMPL_CRATE);
            findings.push(Box::new(
                ExtRowFinding::builder()
                    .rule(self.rule.clone())
                    .disposition(ext_row_disposition(entry.status()))
                    .anchor(anchor)
                    .source_crate(self.upstream_crate.clone())
                    .impl_crate(AMENABLE_EXT_IMPL_CRATE.to_string())
                    .type_path(entry.type_path().clone())
                    .type_kind(entry.type_kind().clone())
                    .is_generic(entry.is_generic())
                    .status(entry.status())
                    .evidence_link(entry.evidence_link())
                    .evidence_name(entry.evidence_name().clone())
                    .kani_witness(entry.kani_witness())
                    .creusot_witness(entry.creusot_witness())
                    .verus_witness(entry.verus_witness())
                    .proof_test(entry.proof_test())
                    .skip_reason(entry.skip_reason().clone())
                    .kani_excepted(entry.kani_excepted())
                    .creusot_excepted(entry.creusot_excepted())
                    .verus_excepted(entry.verus_excepted())
                    .missing_layers(missing_layers)
                    .action(action)
                    .build()?,
            ) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
