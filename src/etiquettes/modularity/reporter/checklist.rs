use std::collections::{BTreeMap, BTreeSet};

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};

use super::super::hierarchy::{build_module_hierarchy, fat_leaves};
use super::rows::{
    HOTSPOT_METHODS, ModularityRow, crate_names, file_module_inputs, is_inventory_row,
    modularity_rows, open_rows, sort_by_lines_desc,
};

use tracing::instrument;
/// Writes `modularity.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularityChecklistReporter;

impl ModularityChecklistReporter {
    pub const ID: &'static str = "modularity-checklist";
}

impl Reporter for ModularityChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let session = view.session;

        let rows = modularity_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let thresholds = *crate::config::load_session_config(session).modularity();
        let inventory_total = open
            .iter()
            .filter(|row| is_inventory_row(row, &thresholds))
            .count();

        let mut body = String::new();
        body.push_str("# Modularity checklist\n\n");
        body.push_str(&format!(
            "Large files/modules (with the longest bodies, packed types, extract-helpers, \
             and whether to grow a subtree named on the same item), function/method bodies >= {} \
             lines, files with more than {} types, parents that kept >= {}% of their \
             subtree, siblings that hold >= {}% of the combined child mass \
             (siblings below {} lines ignored), and unary child directories whose \
             subtree is at least {} lines (collapse the extra hop). Inventory also \
             lists smaller units \
             above the floor (files >= {}, functions/methods >= {}) plus every \
             module's size. Too-long files also name bodies >= {} lines as \
             extract-helpers. File checklist >= {}, module size |z| > {} \
             (upper tail also lines >= {}; {}; modules below {} lines \
             ignored in the sample).\n\n",
            thresholds.function_checklist_min_lines(),
            thresholds.max_types_per_file(),
            thresholds.top_heavy_min_percent(),
            thresholds.lopsided_min_percent(),
            thresholds.hierarchy_min_lines(),
            thresholds.hierarchy_min_lines(),
            thresholds.file_inventory_min_lines(),
            thresholds.function_inventory_min_lines(),
            thresholds.function_hotspot_min_lines(),
            thresholds.file_checklist_min_lines(),
            thresholds.module_size_sigma(),
            thresholds.file_inventory_min_lines(),
            if thresholds.module_size_ignore_lower_tail() {
                "lower tail ignored"
            } else {
                "two-tailed"
            },
            thresholds.min_module_lines(),
        ));

        let mut open_items = 0usize;
        let mut rendered = String::new();
        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let (section, count) = render_crate_checklist(&crate_open);
            if section.is_empty() {
                continue;
            }
            open_items += count;
            rendered.push_str(&format!("## `{crate_name}`\n\n"));
            rendered.push_str(&section);
        }

        body.push_str(&format!("**Open items:** {open_items}\n\n"));
        if rendered.is_empty() {
            body.push_str(&format!(
                "_No checklist items at the current cutoffs ({inventory_total} smaller units remain in \
                 `modularity.csv`)._\n\n"
            ));
        } else {
            body.push_str(&rendered);
        }

        Ok(vec![Box::new(TextArtifact {
            name: "modularity.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

struct FileHotspot<'a> {
    file: &'a str,
    lines: u32,
    zscore: &'a str,
    module: Option<&'a str>,
    methods: Vec<&'a ModularityRow>,
    types: Option<&'a ModularityRow>,
    grow_subtree: bool,
    top_heavy: Option<&'a ModularityRow>,
    lopsided: Option<&'a ModularityRow>,
    collapse: Option<&'a ModularityRow>,
}

#[instrument(level = "debug", skip(open))]
fn render_crate_checklist(open: &[&ModularityRow]) -> (String, usize) {
    let hotspots = file_hotspots(open);
    let nested_files: BTreeSet<&str> = hotspots.iter().map(|hotspot| hotspot.file).collect();
    let nested_methods: BTreeSet<(&str, &str)> = hotspots
        .iter()
        .flat_map(|hotspot| {
            hotspot
                .methods
                .iter()
                .map(|method| (method.file.as_str(), method.context.as_str()))
        })
        .collect();

    let nested_lopsided: BTreeSet<&str> = hotspots
        .iter()
        .filter_map(|hotspot| hotspot.lopsided.map(|row| row.context.as_str()))
        .collect();
    let nested_top_heavy: BTreeSet<&str> = hotspots
        .iter()
        .filter_map(|hotspot| hotspot.top_heavy.map(|row| row.context.as_str()))
        .collect();
    let nested_collapse: BTreeSet<&str> = hotspots
        .iter()
        .filter_map(|hotspot| hotspot.collapse.map(|row| row.context.as_str()))
        .collect();

    let mut leftover_functions: Vec<&ModularityRow> = open
        .iter()
        .copied()
        .filter(|row| {
            row.kind == "MODULARITY-FUNCTION"
                && row.is_checklist()
                && !nested_methods.contains(&(row.file.as_str(), row.context.as_str()))
        })
        .collect();
    sort_by_lines_desc(&mut leftover_functions);

    let mut leftover_types: Vec<&ModularityRow> = open
        .iter()
        .copied()
        .filter(|row| {
            row.kind == "MODULARITY-TYPES-PER-FILE"
                && row.is_checklist()
                && !nested_files.contains(row.file.as_str())
        })
        .collect();
    sort_by_lines_desc(&mut leftover_types);

    let mut body = String::new();
    let mut count = 0usize;

    if !hotspots.is_empty() {
        body.push_str("### Too long\n\n");
        for hotspot in &hotspots {
            count += 1;
            let zscore = if hotspot.zscore.is_empty() {
                String::new()
            } else {
                format!(" (z={})", hotspot.zscore)
            };
            let module = hotspot
                .module
                .map(|path| format!(" `{path}`"))
                .unwrap_or_default();
            body.push_str(&format!(
                "- [ ] `{file}`{module} — **{lines} lines**{zscore}\n",
                file = hotspot.file,
                lines = hotspot.lines,
            ));
            for method in &hotspot.methods {
                body.push_str(&format!(
                    "  - {} `{}` ({} body lines)\n",
                    if method.is_checklist() {
                        "split"
                    } else {
                        "extract helpers from"
                    },
                    method.context,
                    method.lines
                ));
            }
            if hotspot.methods.is_empty() {
                body.push_str(
                    "  - extract helpers — peel predicates, constructors, and shared match arms until this file is under the size cutoff\n",
                );
            }
            if let Some(types) = hotspot.types {
                let names = if types.context.is_empty() {
                    String::new()
                } else {
                    format!(" (`{}`)", types.context)
                };
                body.push_str(&format!(
                    "  - peel types: **{} types**{names}\n",
                    types.lines
                ));
            }
            if hotspot.grow_subtree {
                body.push_str(
                    "  - or grow a subtree if those helpers form a named layer (no child modules yet)\n",
                );
            }
            if let Some(top_heavy) = hotspot.top_heavy {
                body.push_str(&format!(
                    "  - peel the parent — **{} lines** still live here (top-heavy {}){}\n",
                    top_heavy.lines,
                    top_heavy.share,
                    detail_suffix(&top_heavy.detail),
                ));
            }
            if let Some(lopsided) = hotspot.lopsided {
                body.push_str(&format!(
                    "  - split this dominant sibling — {} of sibling mass{}\n",
                    share_label(lopsided),
                    detail_suffix(&lopsided.detail),
                ));
            }
            if let Some(collapse) = hotspot.collapse {
                body.push_str(&format!(
                    "  - collapse this extra hop — **{} lines**{}\n",
                    collapse.lines,
                    detail_suffix(&collapse.detail),
                ));
            }
        }
        body.push('\n');
    }

    if !leftover_functions.is_empty() {
        body.push_str("### Split these bodies\n\n");
        for entry in leftover_functions {
            count += 1;
            body.push_str(&format!(
                "- [ ] `{}` — `{}:{}` — **{} lines** — split this body\n",
                entry.context, entry.file, entry.line, entry.lines
            ));
        }
        body.push('\n');
    }

    if !leftover_types.is_empty() {
        body.push_str("### Packed types\n\n");
        for entry in leftover_types {
            count += 1;
            let names = if entry.context.is_empty() {
                String::new()
            } else {
                format!(" (`{}`)", entry.context)
            };
            body.push_str(&format!(
                "- [ ] `{}` — **{} types**{names}\n",
                entry.file, entry.lines
            ));
        }
        body.push('\n');
    }

    let (rebalance, rebalance_count) =
        render_rebalance(open, &nested_top_heavy, &nested_lopsided, &nested_collapse);
    body.push_str(&rebalance);
    count += rebalance_count;

    (body, count)
}

#[instrument(
    level = "debug",
    skip(open, nested_top_heavy, nested_lopsided, nested_collapse)
)]
fn render_rebalance<'a>(
    open: &[&'a ModularityRow],
    nested_top_heavy: &BTreeSet<&str>,
    nested_lopsided: &BTreeSet<&str>,
    nested_collapse: &BTreeSet<&str>,
) -> (String, usize) {
    let leftover_top_heavy = leftover_kind(open, "MODULARITY-TOP-HEAVY", nested_top_heavy);
    let leftover_lopsided = leftover_kind(open, "MODULARITY-LOPSIDED", nested_lopsided);
    let leftover_collapse = leftover_kind(open, "MODULARITY-COLLAPSE", nested_collapse);
    if leftover_top_heavy.is_empty() && leftover_lopsided.is_empty() && leftover_collapse.is_empty()
    {
        return (String::new(), 0);
    }
    let mut body = String::from("### Rebalance\n\n");
    let mut count = 0usize;
    for entry in leftover_top_heavy {
        count += 1;
        body.push_str(&format!(
            "- [ ] peel `{}` — **{} lines** still in the parent (top-heavy {}){}\n",
            entry.context,
            entry.lines,
            entry.share,
            detail_suffix(&entry.detail),
        ));
    }
    for entry in leftover_lopsided {
        count += 1;
        body.push_str(&format!(
            "- [ ] split `{}` — **{} lines**, {} of sibling mass{}\n",
            entry.context,
            entry.lines,
            share_label(entry),
            detail_suffix(&entry.detail),
        ));
    }
    for entry in leftover_collapse {
        count += 1;
        body.push_str(&format!(
            "- [ ] collapse `{}` — **{} lines**{}\n",
            entry.context,
            entry.lines,
            detail_suffix(&entry.detail),
        ));
    }
    body.push('\n');
    (body, count)
}

#[instrument(level = "debug", skip(open, nested))]
fn leftover_kind<'a>(
    open: &[&'a ModularityRow],
    kind: &str,
    nested: &BTreeSet<&str>,
) -> Vec<&'a ModularityRow> {
    let mut rows: Vec<&ModularityRow> = open
        .iter()
        .copied()
        .filter(|row| {
            row.kind == kind && row.is_checklist() && !nested.contains(row.context.as_str())
        })
        .collect();
    sort_by_lines_desc(&mut rows);
    rows
}

#[instrument(level = "debug", skip(row))]
fn share_label(row: &ModularityRow) -> String {
    if row.share.is_empty() {
        "most".to_string()
    } else {
        format!("{}%", share_as_percent(&row.share))
    }
}

#[instrument(level = "debug")]
fn share_as_percent(share: &str) -> String {
    share
        .parse::<f64>()
        .map(|value| format!("{:.0}", value * 100.0))
        .unwrap_or_else(|_| share.to_string())
}

#[instrument(level = "debug")]
fn detail_suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(" (`{detail}`)")
    }
}

#[instrument(level = "debug", skip(open))]
fn file_hotspots<'a>(open: &[&'a ModularityRow]) -> Vec<FileHotspot<'a>> {
    let tree = build_module_hierarchy(&file_module_inputs(open));
    let fat_leaf_paths: BTreeSet<String> = fat_leaves(&tree)
        .iter()
        .map(|node| node.path.clone())
        .collect();
    let mut by_file: BTreeMap<&str, Vec<&ModularityRow>> = BTreeMap::new();
    for row in open {
        if matches!(
            row.kind.as_str(),
            "MODULARITY-FILE" | "MODULARITY-MODULE-SIZE"
        ) && row.is_checklist()
        {
            by_file.entry(row.file.as_str()).or_default().push(*row);
        }
    }

    let mut hotspots = Vec::new();
    for (file, size_rows) in by_file {
        let file_row = size_rows.iter().find(|row| row.kind == "MODULARITY-FILE");
        let module_row = size_rows
            .iter()
            .filter(|row| row.kind == "MODULARITY-MODULE-SIZE")
            .max_by_key(|row| row.line_count());
        let lines = file_row
            .or(module_row)
            .map(|row| row.line_count())
            .unwrap_or(0);
        let zscore = module_row.map(|row| row.zscore.as_str()).unwrap_or("");
        let mut methods: Vec<&ModularityRow> = open
            .iter()
            .copied()
            .filter(|row| row.kind == "MODULARITY-FUNCTION" && row.file == file)
            .collect();
        sort_by_lines_desc(&mut methods);
        methods.truncate(HOTSPOT_METHODS);
        let types = open.iter().copied().find(|row| {
            row.kind == "MODULARITY-TYPES-PER-FILE" && row.file == file && row.is_checklist()
        });
        let module_path = module_row.map(|row| row.context.as_str());
        let top_heavy = module_path.and_then(|path| {
            open.iter().copied().find(|row| {
                row.kind == "MODULARITY-TOP-HEAVY" && row.context == path && row.is_checklist()
            })
        });
        let lopsided = open.iter().copied().find(|row| {
            row.kind == "MODULARITY-LOPSIDED" && row.file == file && row.is_checklist()
        });
        let collapse = open.iter().copied().find(|row| {
            row.kind == "MODULARITY-COLLAPSE" && row.file == file && row.is_checklist()
        });
        let grow_subtree = module_path
            .map(|path| fat_leaf_paths.contains(path))
            .unwrap_or(false);
        hotspots.push(FileHotspot {
            file,
            lines,
            zscore,
            module: module_path,
            methods,
            types,
            grow_subtree,
            top_heavy,
            lopsided,
            collapse,
        });
    }
    hotspots.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.file.cmp(right.file))
    });
    hotspots
}
