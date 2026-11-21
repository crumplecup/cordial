//! Durable lifecycle metadata beside raw Hotpath reports. No allocation accounting lives here.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Why capture ended. Only normal/error returns include ordinary workload teardown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Workload and ordinary teardown returned successfully.
    Normal,
    /// Workload returned an error, after stack teardown.
    Error,
    /// Stack unwinding ended the owner.
    Panic,
    /// The memory watchdog stopped work; stack teardown was bypassed.
    Watchdog,
    /// State was saved for the next process; stack teardown was bypassed.
    Resume,
    /// The configured budget left no room to work.
    NoRoom,
}

/// Raw report availability, independently of the workload's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    /// Capture has not finished. A dead process in this state is incomplete.
    Running,
    /// A complete Hotpath JSON document was written.
    Complete,
    /// A known exited process did not finish capture.
    Incomplete,
    /// This executable has no Hotpath recorder.
    Disabled,
    /// An expected process/report never appeared.
    Missing,
    /// The report exists but is invalid.
    Malformed,
    /// The JSON document ended before completion.
    Truncated,
    /// Publishing the report failed.
    WriteFailed,
}

/// Provenance supplied by the workload owner; absent fields stay explicitly unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// CLI, isolation driver, isolation child, or test.
    pub role: String,
    /// Cold, warm, or incremental state, when applicable.
    pub state: Option<String>,
    /// Repeat number, when applicable.
    pub repeat: Option<u64>,
    /// Nextest binary identity, without filename escaping.
    pub binary: Option<String>,
    /// Test name, without filename escaping.
    pub test: Option<String>,
    /// Earliest instrumented boundary, including legacy late-start test capture.
    pub boundary: String,
}

/// An expected child, registered durably before spawn. Children write separate manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Child {
    /// Unique process identity, including before an OS pid exists.
    pub id: String,
    /// Restart number, zero for the first child.
    pub restart: usize,
    /// Expected workload identity even if the child never starts capture.
    #[serde(default)]
    pub identity: Identity,
    /// Actual child pid after spawn.
    pub pid: Option<u32>,
    /// Child exit code; absent for a signal or a child still running.
    pub exit_code: Option<i32>,
    /// Whether the parent observed termination.
    pub exited: bool,
    /// Spawn/wait failure, when present.
    pub error: Option<String>,
}

/// One process's durable portion of a run manifest. Each process has one writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Lifecycle schema, independent of Hotpath's report schema.
    pub schema: u32,
    /// Shared identity across the driver and its children.
    pub run_id: String,
    /// Unique identity of this process capture.
    pub process_id: String,
    /// Parent capture identity, if known.
    pub parent: Option<String>,
    /// Number of earlier children in a resumed run.
    pub restart: usize,
    /// OS process id of the producer.
    pub pid: u32,
    /// Executable path of the producer.
    pub executable: Option<PathBuf>,
    /// Source revision observed when this executable was built, absent outside Git.
    #[serde(default)]
    pub producer_revision: Option<String>,
    /// Whether the producer checkout had changes when it was built.
    #[serde(default)]
    pub producer_dirty: Option<bool>,
    /// Workload provenance.
    pub identity: Identity,
    /// Actual Cargo feature flags, as normalized by Cargo's build environment.
    pub features: Vec<String>,
    /// The locally extended dependency, not a claim about an upstream release.
    pub hotpath_version: String,
    /// Whether timing capture was compiled into this producer.
    pub timing_enabled: bool,
    /// Whether allocation capture was compiled into this producer.
    pub allocation_enabled: bool,
    /// Exclusive/inclusive function accounting, or disabled.
    pub accounting: String,
    /// Display metric selected in Hotpath; exact byte/count fields stay separate.
    pub traffic_metric: String,
    /// Time capture began, in nanoseconds since Unix epoch.
    pub started_ns: u128,
    /// Time finalization completed, if any.
    pub finished_ns: Option<u128>,
    /// Finalization reason; missing after an uncatchable termination.
    pub completion: Option<FinishReason>,
    /// Raw report status.
    pub report_status: ReportStatus,
    /// Unique raw report path under the configured store.
    pub report: PathBuf,
    /// Optional additional unique export derived from the configured output path.
    pub export: Option<PathBuf>,
    /// Report staging path, useful if publication was interrupted.
    pub staging: PathBuf,
    /// Expected children, including those that never started capture.
    pub children: Vec<Child>,
    /// Coverage inventory and remaining unverified boundaries.
    #[serde(default)]
    pub boundaries: Vec<super::Boundary>,
    /// Hotpath configuration overrides that may reduce collection or display.
    #[serde(default)]
    pub overrides: std::collections::BTreeMap<String, String>,
    /// Finalization or export error, if any.
    pub error: Option<String>,
}

/// Transferable child registration; contains no profiler state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ticket {
    pub root: PathBuf,
    pub run_id: String,
    pub id: String,
    pub parent: String,
    pub restart: usize,
    pub identity: Identity,
}

/// Owns exactly one Hotpath guard and its publication. Explicit finish is idempotent.
pub struct Capture {
    guard: Option<hotpath::HotpathGuard>,
    root: PathBuf,
    manifest: Manifest,
}

pub(crate) fn unique_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        now(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("manifest has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut staged, value)?;
    staged.flush()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

impl Capture {
    /// Begin before workload setup. Each call reserves a unique run directory.
    pub fn start(root: &Path, identity: Identity) -> io::Result<Self> {
        Self::begin(root, identity, None)
    }

    pub(crate) fn begin(
        root: &Path,
        identity: Identity,
        ticket: Option<Ticket>,
    ) -> io::Result<Self> {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Err(io::Error::other(
                "Hotpath capture already started in this process",
            ));
        }
        let started_ns = now();
        // The guard starts before resolving store configuration and constructing metadata.
        // Keep staging durable across exit/kill; successful publication removes it.
        let staging = tempfile::Builder::new()
            .prefix("cordial-hotpath-")
            .tempdir()?
            .keep()
            .join("hotpath.json");
        let guard = super::guard("cordial", staging.clone());
        let id = ticket
            .as_ref()
            .map_or_else(unique_id, |ticket| ticket.id.clone());
        let run_id = ticket
            .as_ref()
            .map_or_else(|| id.clone(), |ticket| ticket.run_id.clone());
        let root = ticket
            .as_ref()
            .map_or_else(|| root.join(&run_id), |ticket| ticket.root.clone());
        let manifest = Manifest {
            schema: 1,
            boundaries: super::boundaries(),
            overrides: std::env::vars()
                .filter(|(key, _)| {
                    key.starts_with("HOTPATH_")
                        && (key.contains("LIMIT")
                            || key.contains("SAMPLING")
                            || key == "HOTPATH_REPORT"
                            || key == "HOTPATH_FOCUS"
                            || key == "HOTPATH_THREADS_HIDE_INTERNAL")
                })
                .collect(),
            run_id,
            process_id: id.clone(),
            parent: ticket.as_ref().map(|ticket| ticket.parent.clone()),
            restart: ticket.as_ref().map_or(0, |ticket| ticket.restart),
            pid: std::process::id(),
            executable: std::env::current_exe().ok(),
            producer_revision: option_env!("CORDIAL_BUILD_REVISION").map(str::to_owned),
            producer_dirty: option_env!("CORDIAL_BUILD_DIRTY").and_then(|value| value.parse().ok()),
            identity,
            features: env!("CORDIAL_BUILD_FEATURES")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            hotpath_version: "0.31.2+cordial-retained-peak-v1".to_owned(),
            timing_enabled: cfg!(feature = "hotpath"),
            allocation_enabled: cfg!(feature = "hotpath-alloc"),
            accounting: if !cfg!(feature = "hotpath-alloc") {
                "disabled"
            } else if std::env::var("HOTPATH_ALLOC_CUMULATIVE")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            {
                "inclusive"
            } else {
                "exclusive"
            }
            .to_owned(),
            traffic_metric: if cfg!(feature = "hotpath-alloc") {
                std::env::var("HOTPATH_ALLOC_METRIC").unwrap_or_else(|_| "bytes".to_owned())
            } else {
                "disabled".to_owned()
            },
            started_ns,
            finished_ns: None,
            completion: None,
            report_status: ReportStatus::Running,
            report: root.join(format!("{id}.hotpath.json")),
            export: None,
            staging,
            children: Vec::new(),
            error: None,
        };
        let capture = Self {
            guard: Some(guard),
            root,
            manifest,
        };
        capture.save()?;
        Ok(capture)
    }

    /// Current durable record. Calling this does not sample the allocator.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Location of this process's manifest within the shared run directory.
    pub fn manifest_path(&self) -> PathBuf {
        self.root
            .join(format!("{}.manifest.json", self.manifest.process_id))
    }

    fn save(&self) -> io::Result<()> {
        write_json(&self.manifest_path(), &self.manifest)
    }

    /// Resolve store configuration without restarting the guard. Children keep their parent's run directory.
    pub fn configure(
        &mut self,
        root: &Path,
        export: Option<&Path>,
        identity: Identity,
    ) -> io::Result<()> {
        let old = self.manifest_path();
        if self.manifest.parent.is_none() {
            self.root = root.join(&self.manifest.run_id);
            self.manifest.report = self
                .root
                .join(format!("{}.hotpath.json", self.manifest.process_id));
        }
        let previous = &self.manifest.identity;
        self.manifest.identity = Identity {
            state: identity.state.or_else(|| previous.state.clone()),
            repeat: identity.repeat.or(previous.repeat),
            binary: identity.binary.or_else(|| previous.binary.clone()),
            test: identity.test.or_else(|| previous.test.clone()),
            ..identity
        };
        self.manifest.export = export.map(|path| {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            path.with_file_name(format!("{stem}-{}.json", self.manifest.process_id))
        });
        self.save()?;
        if old != self.manifest_path() {
            std::fs::remove_file(old)?;
        }
        Ok(())
    }

    pub(crate) fn expect_child(
        &mut self,
        restart: usize,
        identity: Identity,
    ) -> io::Result<Ticket> {
        let id = unique_id();
        self.manifest.children.push(Child {
            id: id.clone(),
            restart,
            identity: identity.clone(),
            pid: None,
            exit_code: None,
            exited: false,
            error: None,
        });
        self.save()?;
        Ok(Ticket {
            root: self.root.clone(),
            run_id: self.manifest.run_id.clone(),
            id,
            parent: self.manifest.process_id.clone(),
            restart,
            identity,
        })
    }

    pub(crate) fn observe_child(
        &mut self,
        id: &str,
        pid: Option<u32>,
        exit: Option<Option<i32>>,
        error: Option<String>,
    ) -> io::Result<()> {
        let child = self
            .manifest
            .children
            .iter_mut()
            .find(|child| child.id == id)
            .ok_or_else(|| io::Error::other("unregistered profile child"))?;
        child.pid = pid.or(child.pid);
        if let Some(code) = exit {
            child.exited = true;
            child.exit_code = code;
        }
        child.error = error;
        self.save()
    }

    /// Stop once, validate Hotpath's raw document, and publish without overwriting another run.
    pub fn finish(&mut self, reason: FinishReason) -> io::Result<()> {
        let Some(guard) = self.guard.take() else {
            return Ok(());
        };
        drop(guard);
        self.manifest.completion = Some(reason);
        self.manifest.finished_ns = Some(now());
        let published = if cfg!(feature = "hotpath") {
            match report_status(&self.manifest.staging) {
                ReportStatus::Complete => copy_new(&self.manifest.staging, &self.manifest.report)
                    .map(|()| ReportStatus::Complete),
                status => Ok(status),
            }
        } else {
            Ok(ReportStatus::Disabled)
        };
        self.manifest.report_status = match published {
            Ok(status) => status,
            Err(error) => {
                self.manifest.error = Some(error.to_string());
                ReportStatus::WriteFailed
            }
        };
        if self.manifest.report_status == ReportStatus::Complete
            && let Some(export) = &self.manifest.export
            && let Err(error) = copy_new(&self.manifest.report, export)
        {
            self.manifest.error = Some(error.to_string());
        }
        self.save()?;
        if matches!(
            self.manifest.report_status,
            ReportStatus::Complete | ReportStatus::Disabled
        ) {
            let _ = std::fs::remove_file(&self.manifest.staging);
            if let Some(parent) = self.manifest.staging.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
        Ok(())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let reason = if std::thread::panicking() {
            FinishReason::Panic
        } else {
            FinishReason::Normal
        };
        if let Err(error) = self.finish(reason) {
            eprintln!("cordial: could not finish Hotpath manifest: {error}");
        }
    }
}

fn copy_new(source: &Path, target: &Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut input = std::fs::File::open(source)?;
    io::copy(&mut input, &mut output)?;
    output.flush()
}

// Ignore report payloads while validating: finalization must not build a second
// in-memory copy of the raw report when the workload is already near its budget.
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    functions_timing: std::collections::BTreeMap<String, serde::de::IgnoredAny>,
}

fn report_status(path: &Path) -> ReportStatus {
    match std::fs::File::open(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => ReportStatus::Missing,
        Err(_) => ReportStatus::WriteFailed,
        Ok(file) => match serde_json::from_reader::<_, Envelope>(io::BufReader::new(file)) {
            Ok(envelope) if envelope.kind == "hotpath_report" => {
                let _ = envelope.functions_timing;
                ReportStatus::Complete
            }
            Err(error) if error.is_eof() => ReportStatus::Truncated,
            _ => ReportStatus::Malformed,
        },
    }
}

/// A collected process, including an expected child with no manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunReport {
    /// Expected process identity.
    pub process_id: String,
    /// Expected workload identity, including when capture never started.
    pub expected: Option<Identity>,
    /// Durable producer record, absent when capture never started.
    pub manifest: Option<Manifest>,
    /// Parent observation, including pid and spawn/exit errors when capture never began.
    #[serde(default)]
    pub observation: Option<Child>,
    /// Verified current raw report status. Running is always incomplete.
    pub status: ReportStatus,
}

/// Collect the durable run manifest pieces without hiding missing or truncated children.
pub fn read_run(root: &Path) -> io::Result<Vec<RunReport>> {
    let mut records = std::collections::BTreeMap::new();
    let mut standalone = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.to_string_lossy().ends_with(".child.json") {
            match serde_json::from_reader::<_, Child>(io::BufReader::new(std::fs::File::open(
                &path,
            )?)) {
                Ok(child) => standalone.push(child),
                Err(error) => {
                    let id = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .trim_end_matches(".child.json")
                        .to_owned();
                    records.insert(
                        id.clone(),
                        RunReport {
                            process_id: id,
                            expected: None,
                            manifest: None,
                            observation: None,
                            status: if error.is_eof() {
                                ReportStatus::Truncated
                            } else {
                                ReportStatus::Malformed
                            },
                        },
                    );
                }
            }
            continue;
        }
        if !path.to_string_lossy().ends_with(".manifest.json") {
            continue;
        }
        let manifest: Manifest = match serde_json::from_slice(&std::fs::read(&path)?) {
            Ok(manifest) => manifest,
            Err(error) => {
                let id = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .trim_end_matches(".manifest.json")
                    .to_owned();
                records.insert(
                    id.clone(),
                    RunReport {
                        process_id: id,
                        expected: None,
                        manifest: None,
                        observation: None,
                        status: if error.is_eof() {
                            ReportStatus::Truncated
                        } else {
                            ReportStatus::Malformed
                        },
                    },
                );
                continue;
            }
        };
        let status = if manifest.report_status == ReportStatus::Complete {
            report_status(&manifest.report)
        } else {
            manifest.report_status
        };
        records.insert(
            manifest.process_id.clone(),
            RunReport {
                process_id: manifest.process_id.clone(),
                expected: Some(manifest.identity.clone()),
                manifest: Some(manifest),
                observation: None,
                status,
            },
        );
    }
    let children: Vec<_> = records
        .values()
        .filter_map(|record| record.manifest.as_ref())
        .flat_map(|manifest| manifest.children.iter().cloned())
        .collect();
    for child in children.into_iter().chain(standalone) {
        let record = records.entry(child.id.clone()).or_insert(RunReport {
            process_id: child.id.clone(),
            expected: Some(child.identity.clone()),
            manifest: None,
            observation: None,
            status: ReportStatus::Missing,
        });
        if record.expected.is_none() {
            record.expected = Some(child.identity.clone());
        }
        if record.status == ReportStatus::Running && child.exited {
            record.status = ReportStatus::Incomplete;
        }
        record.observation = Some(child);
    }
    Ok(records.into_values().collect())
}
