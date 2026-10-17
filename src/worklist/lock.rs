use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{CordialError, CordialResult, ProgressSink, StoreLayout};

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

    #[instrument(level = "debug", skip(store, progress))]
    fn acquire(
        store: &StoreLayout,
        command: Option<&str>,
        progress: &dyn ProgressSink,
    ) -> CordialResult<Self> {
        std::fs::create_dir_all(store.root())?;
        let path = store.worklist_lock_path();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        let started = Instant::now();
        let mut next_notice = WAIT_NOTICE;
        loop {
            let attempt = if command.is_some() {
                file.try_lock()
            } else {
                file.try_lock_shared()
            };
            match attempt {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    if started.elapsed() >= WAIT_LIMIT {
                        let holder = std::fs::read_to_string(&path).unwrap_or_default();
                        return Err(CordialError::invariant(format!(
                            "timed out waiting for worklist lock held by {}",
                            holder.trim()
                        )));
                    }
                    if started.elapsed() >= next_notice {
                        let holder = std::fs::read_to_string(&path).unwrap_or_default();
                        let holder = holder.lines().next().unwrap_or("another cordial command");
                        progress.notice(format!(
                            "waiting for {holder} ({}s)",
                            started.elapsed().as_secs()
                        ));
                        next_notice += WAIT_NOTICE;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
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
        Ok(Self { _file: file })
    }
}
