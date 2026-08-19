use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct TrenchcoatCsvReporter;

impl TrenchcoatCsvReporter {
    pub const ID: &'static str = "trenchcoat-csv";
}

impl Reporter for TrenchcoatCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,type_path,disposition\n");
        for finding in findings
            .iter()
            .filter(|finding| finding.rule().category() == "trenchcoat")
        {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
                    .unwrap_or("")
            };
            body.push_str(&format!(
                "{},{},{}\n",
                field("crate"),
                field("type_path"),
                finding.disposition()
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "trenchcoats.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}
