//! `cordial perf`: measure cordial's own cost and compare runs.

use clap::Subcommand;
use std::io::{self, Write};
use tracing::instrument;

use crate::CordialResult;
use crate::perf::{PerfStore, RunRecord};

use super::commands::ActCtx;

/// Subcommands of `cordial perf`.
#[derive(Subcommand)]
pub enum PerfCommands {
    /// List the run records kept in this project's store, oldest first.
    List {
        /// Print the records as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

impl PerfCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::List { json } => list(ctx, json),
        }
    }
}

/// One table row: id, subject, state, commit, machine.
#[instrument(level = "trace", skip(record))]
fn row(record: &RunRecord) -> String {
    let sha: String = record.identity().sha().chars().take(8).collect();
    let dirty = if record.identity().dirty() { "+" } else { "" };
    format!(
        "{}  {}  {}  {sha}{dirty}  {}",
        record.id(),
        record.subject(),
        record
            .state()
            .map_or_else(|| "-".to_owned(), |state| state.to_string()),
        record.environment().machine()
    )
}

#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn list(ctx: &ActCtx, json: bool) -> CordialResult<()> {
    let records = PerfStore::new(ctx.store()).list()?;
    let mut output = io::stdout().lock();
    if json {
        writeln!(output, "{}", serde_json::to_string_pretty(&records)?)?;
    } else if records.is_empty() {
        writeln!(output, "No performance records yet.")?;
    } else {
        writeln!(output, "id  subject  state  commit  machine")?;
        for record in &records {
            writeln!(output, "{}", row(record))?;
        }
    }
    Ok(())
}
