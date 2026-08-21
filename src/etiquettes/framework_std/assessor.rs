use crate::error::CordialResult;
use crate::framework_std::{
    FrameworkStdOptions, HOMECOMING_IMPL_CRATE, HOMECOMING_PATCH_SET, HOMECOMING_TRAIT,
    classify_framework_std_row, load_framework_skip_map,
};
use crate::hooks::{AssessView, Assessor};
use crate::ir::collect_trait_impl_type_paths_from_ir;
use crate::objects::{Finding, NodeAnchor};
use crate::store::StoreLayout;

use super::homecoming::{FrameworkStdRowFinding, FrameworkStdRule, homecoming_row_disposition};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct HomecomingStdAssessor;

impl HomecomingStdAssessor {
    pub const ID: &'static str = "homecoming-std-assessor";
}

impl Assessor for HomecomingStdAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &[super::probe::HomecomingStdScopeProbe::ID]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers;
        let ir = view.ir;
        let session = view.session;

        if markers.is_empty() || ir.crate_name() != HOMECOMING_IMPL_CRATE {
            return Ok(Vec::new());
        }

        let store = StoreLayout::from_root(
            session.store_root(),
            crate::store::project_slug_from_path(session.project_root()),
        );
        let skip_map = load_framework_skip_map(&store, HOMECOMING_PATCH_SET);
        let impl_paths = collect_trait_impl_type_paths_from_ir(ir, HOMECOMING_TRAIT);
        let anchor = NodeAnchor(ir.root()?);
        let _options = FrameworkStdOptions::default();

        let mut findings = Vec::new();
        for marker in markers {
            let Some(type_path) = marker.field("type_path") else {
                continue;
            };
            let (trait_status, skip_reason) =
                classify_framework_std_row(type_path, &impl_paths, &skip_map);
            findings.push(Box::new(FrameworkStdRowFinding {
                rule: FrameworkStdRule,
                disposition: homecoming_row_disposition(trait_status),
                anchor,
                source_crate: "std".to_string(),
                trait_name: HOMECOMING_TRAIT.to_string(),
                impl_crate: HOMECOMING_IMPL_CRATE.to_string(),
                type_path: type_path.to_string(),
                type_kind: marker.field("type_kind").unwrap_or("").to_string(),
                is_generic: marker.field("is_generic") == Some("true"),
                trait_status,
                skip_reason,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}

#[cfg(feature = "amenable_std")]
mod amenable {
    use crate::error::CordialResult;
    use crate::framework_std::{
        AMENABLE_IMPL_CRATE, AMENABLE_PATCH_SET, AmenableStdOptions, ClassifyRowArgs,
        amenable_gap_fields, classify_amenable_std_row, collect_proof_chain_subjects,
        ensure_registry_dump_for_assessor, load_merged_std_inventory, load_verifier_skip_map,
    };
    use crate::hooks::{AssessView, Assessor};
    use crate::objects::{Finding, NodeAnchor};
    use crate::store::{StoreLayout, SysrootCache};

    use super::super::amenable::{
        AmenableStdRowFinding, AmenableStdRule, amenable_row_disposition,
    };
    use tracing::instrument;

    #[derive(Debug, Default, Clone, Copy)]
    pub struct AmenableStdAssessor;

    impl AmenableStdAssessor {
        pub const ID: &'static str = "amenable-std-assessor";
    }

    impl Assessor for AmenableStdAssessor {
        #[instrument(level = "trace", skip(self))]
        fn id(&self) -> &str {
            Self::ID
        }

        #[instrument(level = "trace", skip(self))]
        fn consumes(&self) -> &[&str] {
            &[super::super::probe::AmenableStdScopeProbe::ID]
        }

        #[instrument(level = "trace", skip(self, view))]
        fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
            let markers = view.markers;
            let ir = view.ir;
            let session = view.session;

            if markers.is_empty() || ir.crate_name() != AMENABLE_IMPL_CRATE {
                return Ok(Vec::new());
            }

            let store = StoreLayout::from_root(
                session.store_root(),
                crate::store::project_slug_from_path(session.project_root()),
            );
            let sysroot = SysrootCache::default_cache();
            let options = AmenableStdOptions::default();
            let merged_items = load_merged_std_inventory(&sysroot)?;
            let registry =
                ensure_registry_dump_for_assessor(&store, session.project_root(), &options)?;
            let skip_map = load_verifier_skip_map(&store, AMENABLE_PATCH_SET);
            let proof_chain_subjects = collect_proof_chain_subjects(session.project_root())?;
            let anchor = NodeAnchor(ir.root()?);

            let mut findings = Vec::new();
            for marker in markers {
                let Some(type_path) = marker.field("type_path") else {
                    continue;
                };
                let item = merged_items.iter().find(|item| item.path == type_path);
                let entry = classify_amenable_std_row(
                    type_path,
                    ClassifyRowArgs {
                        type_kind: marker.field("type_kind").unwrap_or(""),
                        is_generic: marker.field("is_generic") == Some("true"),
                        alias_target: item.and_then(|item| item.alias_target.as_deref()),
                        items: &merged_items,
                        registry: &registry,
                        skip_map: &skip_map,
                        proof_chain_subjects: &proof_chain_subjects,
                    },
                );
                let (missing_layers, action) = amenable_gap_fields(&entry, AMENABLE_IMPL_CRATE);
                findings.push(Box::new(AmenableStdRowFinding {
                    rule: AmenableStdRule,
                    disposition: amenable_row_disposition(entry.status),
                    anchor,
                    source_crate: "std".to_string(),
                    impl_crate: AMENABLE_IMPL_CRATE.to_string(),
                    type_path: entry.type_path,
                    type_kind: entry.type_kind,
                    is_generic: entry.is_generic,
                    status: entry.status,
                    evidence_link: entry.evidence_link,
                    evidence_name: entry.evidence_name,
                    kani_witness: entry.kani_witness,
                    creusot_witness: entry.creusot_witness,
                    verus_witness: entry.verus_witness,
                    proof_test: entry.proof_test,
                    skip_reason: entry.skip_reason,
                    kani_excepted: entry.kani_excepted,
                    creusot_excepted: entry.creusot_excepted,
                    verus_excepted: entry.verus_excepted,
                    missing_layers,
                    action,
                }) as Box<dyn Finding>);
            }
            Ok(findings)
        }
    }
}

#[cfg(feature = "amenable_std")]
pub use amenable::AmenableStdAssessor;
