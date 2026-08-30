use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};

use super::super::hierarchy::build_module_hierarchy;
use super::rows::{
    ModularityRow, crate_names, file_module_inputs, is_inventory_row, modularity_rows, open_rows,
    sort_by_lines_desc,
};

use tracing::instrument;
/// Writes `modularity.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularityCsvReporter;

impl ModularityCsvReporter {
    pub const ID: &'static str = "modularity-csv";
}

impl Reporter for ModularityCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let session = view.session;

        let all_rows = modularity_rows(findings);
        let thresholds = *crate::config::load_session_config(session).modularity();
        let mut rows: Vec<_> = open_rows(&all_rows)
            .filter(|row| is_inventory_row(row, &thresholds))
            .collect();
        sort_by_lines_desc(&mut rows);

        let mut body =
            String::from("crate,kind,context,file,line,lines,checklist,zscore,share,detail\n");
        for row in rows {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.kind),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.lines),
                csv_field(&row.checklist),
                csv_field(&row.zscore),
                csv_field(&row.share),
                csv_field(&row.detail),
            ));
        }
        let mut artifacts: Vec<Box<dyn Artifact>> = vec![Box::new(TextArtifact {
            name: "modularity.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })];
        artifacts.push(branches_csv_artifact(&all_rows));
        Ok(artifacts)
    }
}

#[instrument(level = "debug", skip(all_rows))]
fn branches_csv_artifact(all_rows: &[ModularityRow]) -> Box<dyn Artifact> {
    let open: Vec<&ModularityRow> = open_rows(all_rows).collect();
    let mut body =
        String::from("crate,module,file,order,depth,own_lines,subtree_lines,top_heavy,children\n");
    for crate_name in crate_names(&open) {
        let crate_rows: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| row.crate_name == crate_name)
            .collect();
        let mut nodes = build_module_hierarchy(&file_module_inputs(&crate_rows));
        nodes.sort_by(|left, right| {
            right
                .top_heavy()
                .total_cmp(&left.top_heavy())
                .then_with(|| right.own_lines.cmp(&left.own_lines))
                .then_with(|| left.path.cmp(&right.path))
        });
        for node in nodes {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{:.3},{}\n",
                csv_field(&crate_name),
                csv_field(&node.path),
                csv_field(&node.file),
                node.order,
                node.depth,
                node.own_lines,
                node.subtree_lines,
                node.top_heavy(),
                node.child_count,
            ));
        }
    }
    Box::new(TextArtifact {
        name: "modularity-branches.csv".to_string(),
        media_type: "text/csv".to_string(),
        body,
    })
}
