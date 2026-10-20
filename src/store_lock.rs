use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{CordialError, CordialResult, ProgressSink, StoreLayout, SysrootCache};

use tracing::instrument;
const WAIT_NOTICE: Duration = Duration::from_secs(10);
const WAIT_LIMIT: Duration = Duration::from_secs(30 * 60);

/// Advisory store lock held for the complete gate run or list update.
pub struct WorklistLock {
    _file: File,
}

impl WorklistLock {
    /// Wait for exclusive access to the worklist store and record the holder.
    #[instrument(level = "debug", skip(store, progress))]
    pub fn exclusive(
        store: &StoreLayout,
        command: &str,
        progress: &dyn ProgressSink,
    ) -> CordialResult<Self> {
        Self::acquire(store, Some(command), progress)
    }

    /// Wait for shared access while another command may be replacing the list.
    #[instrument(level = "debug", skip(store, progress))]
    pub fn shared(store: &StoreLayout, progress: &dyn ProgressSink) -> CordialResult<Self> {
        Self::acquire(store, None, progress)
    }

    /// Attempt exclusive access without waiting; `None` means another command holds it.
    #[instrument(level = "debug", skip(store))]
    pub fn try_exclusive(store: &StoreLayout, command: &str) -> CordialResult<Option<Self>> {
        let file = acquire_lock(&store.worklist_lock_path(), Some(command), None, false)?;
        Ok(file.map(|file| Self { _file: file }))
    }

    #[instrument(level = "debug", skip(store, progress))]
    fn acquire(
        store: &StoreLayout,
        command: Option<&str>,
        progress: &dyn ProgressSink,
    ) -> CordialResult<Self> {
        let file = acquire_lock(
            &store.worklist_lock_path(),
            command,
            Some(progress),
            command.is_none(),
        )?
        .ok_or_else(|| CordialError::invariant("waiting lock acquisition returned busy"))?;
        Ok(Self { _file: file })
    }
}

/// Advisory lock for the shared sysroot cache.
pub struct SysrootLock {
    _file: File,
}

impl SysrootLock {
    /// Wait for exclusive access while building shared sysroot artifacts.
    #[instrument(level = "debug", skip(sysroot, progress))]
    pub fn exclusive(
        sysroot: &SysrootCache,
        command: &str,
        progress: &dyn ProgressSink,
    ) -> CordialResult<Self> {
        let file = acquire_lock(&sysroot.lock_path(), Some(command), Some(progress), false)?
            .ok_or_else(|| {
                CordialError::invariant("waiting sysroot lock acquisition returned busy")
            })?;
        Ok(Self { _file: file })
    }

    /// Attempt exclusive access without waiting during `clean --all`.
    #[instrument(level = "debug", skip(sysroot))]
    pub fn try_exclusive(sysroot: &SysrootCache, command: &str) -> CordialResult<Option<Self>> {
        let file = acquire_lock(&sysroot.lock_path(), Some(command), None, false)?;
        Ok(file.map(|file| Self { _file: file }))
    }
}

#[instrument(level = "debug", skip(path, progress), err(level = "warn"))]
fn acquire_lock(
    path: &Path,
    command: Option<&str>,
    progress: Option<&dyn ProgressSink>,
    shared: bool,
) -> CordialResult<Option<File>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    let wait = progress.is_some();
    if shared && command.is_some() {
        return Err(CordialError::invariant(
            "shared lock cannot record an exclusive command",
        ));
    }
    {
        let started = Instant::now();
        let mut next_notice = WAIT_NOTICE;
        loop {
            let attempt = if shared {
                file.try_lock_shared()
            } else {
                file.try_lock()
            };
            match attempt {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    if !wait {
                        return Ok(None);
                    }
                    if started.elapsed() >= WAIT_LIMIT {
                        let holder = std::fs::read_to_string(path).unwrap_or_default();
                        return Err(CordialError::invariant(format!(
                            "timed out waiting for store lock held by {}",
                            holder.trim()
                        )));
                    }
                    if started.elapsed() >= next_notice {
                        let holder = std::fs::read_to_string(path).unwrap_or_default();
                        let holder = holder.lines().next().unwrap_or("another cordial command");
                        if let Some(progress) = progress {
                            progress.notice(format!(
                                "waiting for {holder} ({}s)",
                                started.elapsed().as_secs()
                            ));
                        }
                        next_notice += WAIT_NOTICE;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    }
    if let Some(command) = command {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CordialError::invariant(error.to_string()))?
            .as_secs();
        writeln!(file, "{command}\nstarted {start_time}")?;
        file.sync_all()?;
    }
    Ok(Some(file))
}
