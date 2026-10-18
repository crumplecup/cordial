//! Distinguish fresh area results from earlier checklists and ledger counts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tracing::instrument;

use crate::{CordialConfig, GateLedger, assigned_tier};

use super::QualityReport;

/// Compiled etiquettes whose findings contribute to an area.
#[instrument(level = "debug")]
pub(crate) fn quality_area_owners(title: &str) -> Vec<&'static str> {
    crate::etiquettes::quality_report_etiquettes()
        .into_iter()
        .filter(|etiquette| {
            if title == "Error handling" {
                matches!(
                    etiquette.id(),
                    "panics"
                        | "internal_error_chain"
                        | "foreign_error_attenuation"
                        | "antipatterns"
                )
            } else {
                etiquette
                    .quality_area()
                    .is_some_and(|area| area.title() == title)
            }
        })
        .map(|etiquette| etiquette.id())
        .collect()
}

#[derive(Debug, Default)]
pub(super) struct ReportRunCoverage {
    areas: BTreeMap<String, AreaRunCoverage>,
    stale_checklists: BTreeSet<String>,
}

#[derive(Debug, derive_getters::Getters)]
pub(super) struct AreaRunCoverage {
    status: String,
    #[getter(copy)]
    previous_open: Option<usize>,
    #[getter(copy)]
    ran: bool,
}

impl ReportRunCoverage {
    #[instrument(level = "debug", skip(report, config, ledger, fresh_artifacts))]
    pub(super) fn new(
        report: &QualityReport,
        active_ids: &[&str],
        config: &CordialConfig,
        ledger: &GateLedger,
        fresh_artifacts: &BTreeSet<String>,
        findings_dir: &Path,
    ) -> Self {
        let mut coverage = Self::default();
        for area in report.areas() {
            let owners = quality_area_owners(area.title());
            let skipped: Vec<_> = owners
                .iter()
                .filter(|id| !active_ids.contains(id))
                .filter_map(|id| {
                    crate::etiquettes::quality_report_etiquettes()
                        .into_iter()
                        .find(|etiquette| etiquette.id() == *id)
                        .map(|etiquette| {
                            format!("{}: {}", id, assigned_tier(etiquette, config).as_str())
                        })
                })
                .collect();
            if !skipped.is_empty() {
                let ran = owners.iter().any(|id| active_ids.contains(id));
                let label = if ran { "partially run" } else { "not run" };
                coverage.areas.insert(
                    area.title().clone(),
                    AreaRunCoverage {
                        status: format!("{label} (tier checks skipped: {})", skipped.join(", ")),
                        previous_open: ledger.area_open_counts().get(area.title()).copied(),
                        ran,
                    },
                );
            }
            for checklist in area.checklists() {
                if !fresh_artifacts.contains(checklist.name())
                    && findings_dir.join(checklist.name()).is_file()
                {
                    coverage.stale_checklists.insert(checklist.name().clone());
                }
            }
        }
        coverage
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn area(&self, title: &str) -> Option<&AreaRunCoverage> {
        self.areas.get(title)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn checklist_is_stale(&self, name: &str) -> bool {
        self.stale_checklists.contains(name)
    }
}
