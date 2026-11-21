//! The existing snapshot wire format streamed from borrowed graph payloads.

use super::{CrateIr, IO_BUFFER_BYTES, ZSTD_LEVEL};
use crate::CordialResult;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};
use std::fs;
use std::path::Path;

/// References only: no node, edge, attribute or index payload is copied.
pub(super) struct BorrowedSnapshot<'a>(pub(super) &'a CrateIr);

impl Serialize for BorrowedSnapshot<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut snapshot = serializer.serialize_struct("CrateIrSnapshot", 5)?;
        snapshot.serialize_field("crate_name", &self.0.crate_name)?;
        snapshot.serialize_field("root", &self.0.root)?;
        snapshot.serialize_field("nodes", &Nodes(self.0))?;
        snapshot.serialize_field("edges", &Edges(self.0))?;
        snapshot.serialize_field("indexes", &self.0.indexes)?;
        snapshot.end()
    }
}

struct Nodes<'a>(&'a CrateIr);
impl Serialize for Nodes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut nodes = serializer.serialize_seq(Some(self.0.graph.node_count()))?;
        for node in self.0.graph.node_weights() {
            nodes.serialize_element(node)?;
        }
        nodes.end()
    }
}

struct Edges<'a>(&'a CrateIr);
impl Serialize for Edges<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut edges = serializer.serialize_seq(Some(self.0.graph.edge_count()))?;
        for edge in self.0.graph.edge_references() {
            edges.serialize_element(&(
                edge.source().index() as u32,
                edge.target().index() as u32,
                edge.weight(),
            ))?;
        }
        edges.end()
    }
}

#[tracing::instrument(level = "info", skip(path, value), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn write_cache(
    path: &Path,
    value: &impl Serialize,
    nodes: usize,
    edges: usize,
) -> CordialResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let started = std::time::Instant::now();
    let encoder = zstd::stream::write::Encoder::new(fs::File::create(path)?, ZSTD_LEVEL)?;
    let mut out = std::io::BufWriter::with_capacity(IO_BUFFER_BYTES, encoder);
    serde_json::to_writer(&mut out, value)?;
    out.into_inner()
        .map_err(std::io::IntoInnerError::into_error)?
        .finish()?;
    tracing::debug!(
        target: "cordial::metrics",
        path = %path.display(),
        bytes = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
        nodes,
        edges,
        millis = started.elapsed().as_millis() as u64,
        "ir file written"
    );
    Ok(())
}
