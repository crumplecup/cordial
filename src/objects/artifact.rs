use std::fmt::Display;
use std::io::Write;

use crate::error::CordialResult;
use crate::objects::IrAnchor;

use tracing::instrument;
/// Structured field sink for format-evolvable findings.
pub trait FindingSink {
    /// Record a named field on this finding.
    fn field(&mut self, name: &str, value: &dyn Display);
    /// Record a source snippet on this finding.
    fn snippet(&mut self, source: &str);
    /// Record a related IR anchor.
    fn related(&mut self, anchor: &dyn IrAnchor);
}

/// Rendered output produced by a reporter.
pub trait Artifact: Send + Sync {
    /// Filename written under the store (for example `crate-attrs.csv`).
    fn name(&self) -> &str;
    /// IANA media type of the artifact body.
    fn media_type(&self) -> &str;
    /// Write the artifact body to `dest`.
    fn write_to(&self, dest: &mut dyn Write) -> CordialResult<()>;
}

/// Collects finding fields into a flat map for default reporters.
#[derive(Debug, Default, Clone, derive_getters::Getters)]
pub struct MapFindingSink {
    /// Collected named finding fields.
    fields: Vec<(String, String)>,
    /// Collected source snippets.
    snippets: Vec<String>,
    /// Collected related-anchor displays.
    related: Vec<String>,
}

impl MapFindingSink {
    /// Consume the sink into its collected named fields.
    #[instrument(level = "debug", skip(self))]
    pub fn into_fields(self) -> Vec<(String, String)> {
        self.fields
    }
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
#[derive(Debug, Clone, derive_new::new)]
pub struct TextArtifact {
    /// Filename written under the store (for example `crate-attrs.csv`).
    name: String,
    /// IANA media type of the artifact body.
    media_type: String,
    /// Artifact payload.
    body: String,
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
