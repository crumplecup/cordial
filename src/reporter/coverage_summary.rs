use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::{Artifact, Finding};
use crate::plugin::{Plugin, PluginCategory, plugins_in_category, selected_plugins};
use crate::session::{RunFilter, SessionView};

use tracing::instrument;
/// One registered coverage plugin section in the workspace rollup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoveragePluginSummary {
    /// Coverage plugin identifier.
    pub plugin_id: String,
    /// Human-readable coverage plugin name.
    pub plugin_name: String,
    /// Artifact payload.
    pub body: String,
}

/// Workspace rollup across registered coverage plugins.
pub struct CoverageSummary {
    /// Per-plugin coverage summaries.
    pub plugins: Vec<CoveragePluginSummary>,
    /// Additional artifacts emitted beside the summary.
    pub extra_artifacts: Vec<Box<dyn Artifact>>,
}

impl std::fmt::Debug for CoverageSummary {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverageSummary")
            .field("plugins", &self.plugins)
            .field("extra_artifacts_len", &self.extra_artifacts.len())
            .finish()
    }
}

/// Build a coverage rollup for the selected coverage plugins in this run.
#[instrument(
    level = "debug",
    skip(registered_plugins, filter, session, findings, workspace),
    err(level = "warn")
)]
pub fn build_coverage_summary(
    registered_plugins: &[&'static dyn Plugin],
    resolved_etiquette_ids: &[&str],
    filter: &dyn RunFilter,
    session: &dyn SessionView,
    findings: &[&dyn Finding],
    workspace: &WorkspaceIr,
) -> CordialResult<CoverageSummary> {
    let coverage_plugins = coverage_plugins_for_run(registered_plugins, filter);
    let mut plugins = Vec::new();
    let mut extra_artifacts = Vec::new();
    if !coverage_plugins.is_empty() {
        for plugin in coverage_plugins {
            let section = section_for_plugin(plugin, session, filter, findings, workspace)?;
            plugins.push(section.summary);
            extra_artifacts.extend(section.extra_artifacts);
        }
    } else {
        let mut saw_elicitation = false;
        for etiquette_id in resolved_etiquette_ids {
            match *etiquette_id {
                #[cfg(feature = "homecoming_std")]
                "homecoming-std" => plugins.push(CoveragePluginSummary {
                    plugin_id: "homecoming-std".to_string(),
                    plugin_name: "Homecoming std coverage".to_string(),
                    body: homecoming_section::homecoming_std_section(findings)?,
                }),
                #[cfg(feature = "amenable_std")]
                "amenable-std" => plugins.push(CoveragePluginSummary {
                    plugin_id: "amenable-std".to_string(),
                    plugin_name: "Amenable std coverage".to_string(),
                    body: amenable_section::amenable_std_section(findings)?,
                }),
                #[cfg(feature = "elicitation")]
                "impl-coverage" | "trenchcoat" | "shadow" if !saw_elicitation => {
                    saw_elicitation = true;
                    let rollup = elicitation_section_impl::elicitation_section(
                        session, filter, findings, workspace,
                    )?;
                    plugins.push(rollup.summary);
                    extra_artifacts.extend(rollup.extra_artifacts);
                }
                _ => {}
            }
        }
    }
    Ok(CoverageSummary {
        plugins,
        extra_artifacts,
    })
}

struct CoverageSection {
    summary: CoveragePluginSummary,
    extra_artifacts: Vec<Box<dyn Artifact>>,
}

#[instrument(
    level = "debug",
    skip(plugin, session, filter, findings, workspace),
    err(level = "warn")
)]
fn section_for_plugin(
    plugin: &dyn Plugin,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
    findings: &[&dyn Finding],
    workspace: &WorkspaceIr,
) -> CordialResult<CoverageSection> {
    match plugin.id() {
        #[cfg(feature = "homecoming_std")]
        "homecoming-std-coverage" => Ok(CoverageSection {
            summary: CoveragePluginSummary {
                plugin_id: plugin.id().to_string(),
                plugin_name: plugin.name().to_string(),
                body: homecoming_section::homecoming_std_section(findings)?,
            },
            extra_artifacts: Vec::new(),
        }),
        #[cfg(feature = "amenable_std")]
        "amenable-std-coverage" => Ok(CoverageSection {
            summary: CoveragePluginSummary {
                plugin_id: plugin.id().to_string(),
                plugin_name: plugin.name().to_string(),
                body: amenable_section::amenable_std_section(findings)?,
            },
            extra_artifacts: Vec::new(),
        }),
        #[cfg(feature = "elicitation")]
        "elicitation-coverage" => {
            elicitation_section_impl::elicitation_section(session, filter, findings, workspace)
        }
        other => Ok(CoverageSection {
            summary: CoveragePluginSummary {
                plugin_id: plugin.id().to_string(),
                plugin_name: plugin.name().to_string(),
                body: format!("Coverage plugin `{other}` has no summary section yet.\n"),
            },
            extra_artifacts: Vec::new(),
        }),
    }
}

/// Render coverage summary markdown.
#[instrument(level = "debug", skip(summary), err(level = "warn"))]
pub fn render_coverage_summary_markdown(summary: &CoverageSummary) -> CordialResult<String> {
    let mut out = String::new();
    writeln!(out, "# Coverage summary")?;
    writeln!(out)?;
    if summary.plugins.is_empty() {
        writeln!(out, "_No coverage plugins ran._")?;
        return Ok(out);
    }
    writeln!(
        out,
        "Rollup for **{}** registered coverage plugin(s).\n",
        summary.plugins.len()
    )?;

    for (index, plugin) in summary.plugins.iter().enumerate() {
        if index > 0 {
            writeln!(out, "\n---\n")?;
        }
        writeln!(out, "## {}\n", plugin.plugin_name)?;
        out.push_str(&plugin.body);
        if !plugin.body.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

#[instrument(level = "debug", skip(registered_plugins, filter))]
pub fn coverage_plugins_for_run(
    registered_plugins: &[&'static dyn Plugin],
    filter: &dyn RunFilter,
) -> Vec<&'static dyn Plugin> {
    if registered_plugins.is_empty() {
        return Vec::new();
    }
    let selected = selected_plugins(registered_plugins, filter.plugins());
    let mut coverage = plugins_in_category(&selected, PluginCategory::Coverage);
    if let Some(ids) = filter.etiquettes() {
        coverage.retain(|plugin| {
            plugin
                .etiquettes()
                .iter()
                .any(|etiquette| ids.iter().any(|id| id == etiquette.id()))
        });
    }
    coverage
}

/// Per-coverage-plugin summary section builders, each gated as a whole unit
/// — see `docs/planning/cfg-scatter-etiquette.md` for the pattern. The
/// dispatch match arms above still need one `#[cfg]` apiece (they key off
/// two different id spaces), but the section bodies live here.
#[cfg(feature = "homecoming_std")]
mod homecoming_section {
    use crate::error::CordialResult;
    use crate::objects::Finding;
    use tracing::instrument;

    #[instrument(level = "debug", skip(findings), err(level = "warn"))]
    pub(super) fn homecoming_std_section(findings: &[&dyn Finding]) -> CordialResult<String> {
        use crate::etiquettes::framework_std::framework_report_from_findings;
        use crate::framework_std::{FrameworkStdOptions, render_framework_summary_md};

        let options = FrameworkStdOptions::default();
        let Some(report) = framework_report_from_findings(findings, options.include_nightly) else {
            return Ok("_No homecoming std findings._\n".to_string());
        };
        Ok(render_framework_summary_md(&report))
    }
}

#[cfg(feature = "amenable_std")]
mod amenable_section {
    use crate::error::CordialResult;
    use crate::objects::Finding;
    use tracing::instrument;

    #[instrument(level = "debug", skip(findings), err(level = "warn"))]
    pub(super) fn amenable_std_section(findings: &[&dyn Finding]) -> CordialResult<String> {
        use crate::etiquettes::framework_std::amenable_report_from_findings;
        use crate::framework_std::{AmenableStdOptions, render_amenable_std_summary_md};

        let options = AmenableStdOptions::default();
        let Some(report) = amenable_report_from_findings(findings, options.include_nightly) else {
            return Ok("_No amenable std findings._\n".to_string());
        };
        Ok(render_amenable_std_summary_md(&report))
    }
}

#[cfg(feature = "elicitation")]
mod elicitation_section_impl {
    use crate::error::CordialResult;
    use crate::ir::WorkspaceIr;
    use crate::objects::Finding;
    use crate::session::{RunFilter, SessionView};
    use tracing::instrument;

    use super::{CoveragePluginSummary, CoverageSection};

    #[instrument(
        level = "debug",
        skip(session, filter, findings, workspace),
        err(level = "warn")
    )]
    pub(super) fn elicitation_section(
        session: &dyn SessionView,
        filter: &dyn RunFilter,
        findings: &[&dyn Finding],
        workspace: &WorkspaceIr,
    ) -> CordialResult<CoverageSection> {
        let rollup = crate::reporter::elicitation_summary::build_elicitation_coverage_rollup(
            session, filter, findings, workspace,
        )?;
        Ok(CoverageSection {
            summary: CoveragePluginSummary {
                plugin_id: "elicitation-coverage".to_string(),
                plugin_name: "Elicitation coverage".to_string(),
                body: rollup.body,
            },
            extra_artifacts: rollup.extra_artifacts,
        })
    }
}
