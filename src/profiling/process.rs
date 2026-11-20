//! Process ownership shared with the existing watchdog and isolation driver.

use super::capture::{Ticket, unique_id};
use super::{Capture, FinishReason, Identity};
use std::io;
use std::path::Path;

pub(crate) const TICKET_VARIABLE: &str = "CORDIAL_PROFILE_TICKET";
static HELD: std::sync::Mutex<Option<Capture>> = std::sync::Mutex::new(None);

/// Outer process owner. Start before parsing/tracing; finish after workload teardown.
pub struct ProcessProfile {
    id: String,
}

impl ProcessProfile {
    /// Begin early, using a fallback store until the CLI resolves configuration.
    pub fn start() -> io::Result<Self> {
        let ticket = std::env::var(TICKET_VARIABLE)
            .ok()
            .map(|text| serde_json::from_str::<Ticket>(&text))
            .transpose()?;
        let project = std::env::current_dir()?;
        let root = crate::default_store_home()
            .join(crate::project_slug_from_path(&project))
            .join("perf/raw/profiles");
        let identity = ticket.as_ref().map_or_else(
            || Identity {
                role: "cli".to_owned(),
                boundary: "process_entry".to_owned(),
                ..Identity::default()
            },
            |ticket| ticket.identity.clone(),
        );
        Self::start_at(&root, identity, ticket)
    }

    /// Begin a controlled process fixture at an explicit store location.
    pub fn start_in(root: &Path, identity: Identity) -> io::Result<Self> {
        Self::start_at(root, identity, None)
    }

    fn start_at(root: &Path, identity: Identity, ticket: Option<Ticket>) -> io::Result<Self> {
        let mut held = HELD
            .lock()
            .map_err(|_| io::Error::other("profile owner poisoned"))?;
        if held.is_some() {
            return Err(io::Error::other("a process already owns Hotpath capture"));
        }
        let capture = Capture::begin(root, identity, ticket)?;
        let owner = Self {
            id: capture.manifest().process_id.clone(),
        };
        *held = Some(capture);
        Ok(owner)
    }

    /// Explicit controlled exit. Repeated watchdog/owner finishes have no effect.
    pub fn finish(self, reason: FinishReason) {
        finish_owned(Some(&self.id), reason);
    }
}

impl Drop for ProcessProfile {
    fn drop(&mut self) {
        finish_owned(
            Some(&self.id),
            if std::thread::panicking() {
                FinishReason::Panic
            } else {
                FinishReason::Normal
            },
        );
    }
}

fn finish_owned(id: Option<&str>, reason: FinishReason) {
    // Serialize publication with every controlled-exit caller. A watchdog must
    // not exit the process while the ordinary owner is still writing its report.
    if let Ok(mut held) = HELD.lock()
        && id.is_none_or(|id| {
            held.as_ref()
                .is_some_and(|capture| capture.manifest().process_id == id)
        })
        && let Some(mut capture) = held.take()
        && let Err(error) = capture.finish(reason)
    {
        eprintln!("cordial: could not finish Hotpath capture: {error}");
    }
}

pub(crate) fn finish(reason: FinishReason) {
    finish_owned(None, reason);
}

#[cfg(feature = "cli")]
pub(crate) fn configure(root: &Path, export: Option<&Path>, identity: Identity) -> io::Result<()> {
    let mut held = HELD
        .lock()
        .map_err(|_| io::Error::other("profile owner poisoned"))?;
    if let Some(capture) = held.as_mut() {
        capture.configure(root, export, identity)?;
    }
    Ok(())
}

pub(crate) fn expect_child(restart: usize) -> io::Result<Option<ChildProfile>> {
    expect_child_for(restart, None)
}

pub(crate) fn expect_child_for(
    restart: usize,
    identity: Option<Identity>,
) -> io::Result<Option<ChildProfile>> {
    let mut held = HELD
        .lock()
        .map_err(|_| io::Error::other("profile owner poisoned"))?;
    held.as_mut()
        .map(|capture| {
            let identity = identity.unwrap_or_else(|| Identity {
                role: "isolation_child".to_owned(),
                ..capture.manifest().identity.clone()
            });
            capture
                .expect_child(restart, identity)
                .map(|ticket| ChildProfile { ticket })
        })
        .transpose()
}

/// Updates only the parent's manifest. The child owns its report and manifest.
pub(crate) struct ChildProfile {
    ticket: Ticket,
}

impl ChildProfile {
    pub(crate) fn environment(&self) -> io::Result<(&'static str, String)> {
        Ok((TICKET_VARIABLE, serde_json::to_string(&self.ticket)?))
    }

    pub(crate) fn observe(
        &self,
        pid: Option<u32>,
        exit: Option<Option<i32>>,
        error: Option<String>,
    ) -> io::Result<()> {
        let mut held = HELD
            .lock()
            .map_err(|_| io::Error::other("profile owner poisoned"))?;
        if let Some(capture) = held.as_mut() {
            capture.observe_child(&self.ticket.id, pid, exit, error)?;
        }
        Ok(())
    }
}

/// Reserve a unique directory for per-test reports, including sibling runs in the same second.
pub fn test_run_directory(root: &Path) -> io::Result<std::path::PathBuf> {
    let path = root.join(unique_id());
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
