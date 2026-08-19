use std::fmt::Display;
use std::io::Write;

use crate::error::CordialResult;
use crate::objects::IrAnchor;

use tracing::instrument;
/// Structured field sink for format-evolvable findings.
pub trait FindingSink {
    fn field(&mut self, name: &str, value: &dyn Display);
    fn snippet(&mut self, source: &str);
    fn related(&mut self, anchor: &dyn IrAnchor);
}

/// Rendered output produced by a reporter.
pub trait Artifact: Send + Sync {
    fn name(&self) -> &str;
    fn media_type(&self) -> &str;
    fn write_to(&self, dest: &mut dyn Write) -> CordialResult<()>;
}

/// Collects finding fields into a flat map for default reporters.
#[derive(Debug, Default, Clone)]
pub struct MapFindingSink {
    pub fields: Vec<(String, String)>,
    pub snippets: Vec<String>,
    pub related: Vec<String>,
}

impl FindingSink for MapFindingSink {
    #[instrument(level = "trace", skip(self, value))]
    fn field(&mut self, name: &str, value: &dyn Display) {
        self.fields.push((name.to_string(), value.to_string()));
    }

    #[instrument(level = "trace", skip(self, source))]
    fn snippet(&mut self, source: &str) {
        self.snippets.push(source.to_string());
    }

    #[instrument(level = "trace", skip(self, anchor))]
    fn related(&mut self, anchor: &dyn IrAnchor) {
        self.related.push(anchor.node_id().to_string());
    }
}

/// UTF-8 artifact backed by an in-memory buffer.
#[derive(Debug, Clone)]
pub struct TextArtifact {
    pub name: String,
    pub media_type: String,
    pub body: String,
}

impl Artifact for TextArtifact {
    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        &self.name
    }

    #[instrument(level = "trace", skip(self))]
    fn media_type(&self) -> &str {
        &self.media_type
    }

    #[instrument(level = "info", skip(self, dest), err(level = "warn"))]
    fn write_to(&self, dest: &mut dyn Write) -> CordialResult<()> {
        dest.write_all(self.body.as_bytes())?;
        Ok(())
    }
}
