//! Native sources for the advisory file-lock machinery in `store_lock.rs`.

use std::panic::Location;

use tracing::instrument;

#[derive(Debug, derive_getters::Getters)]
pub struct WaitingLockBusySource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WaitingLockBusySource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WaitingLockBusySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "waiting lock acquisition returned busy ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for WaitingLockBusySource {}

#[derive(Debug, derive_getters::Getters)]
pub struct WaitingSysrootLockBusySource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl WaitingSysrootLockBusySource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for WaitingSysrootLockBusySource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "waiting sysroot lock acquisition returned busy ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for WaitingSysrootLockBusySource {}

#[derive(Debug, derive_getters::Getters)]
pub struct SharedLockExclusiveCommandSource {
    file: String,
    #[getter(copy)]
    line: u32,
}

impl SharedLockExclusiveCommandSource {
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        let loc = Location::caller();
        Self {
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for SharedLockExclusiveCommandSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "shared lock cannot record an exclusive command ({}:{})",
            self.file, self.line
        )
    }
}

impl std::error::Error for SharedLockExclusiveCommandSource {}

#[derive(Debug, derive_getters::Getters)]
pub struct StoreLockTimedOutSource {
    #[getter(skip)]
    holder: String,
    file: String,
    #[getter(copy)]
    line: u32,
}

impl StoreLockTimedOutSource {
    #[track_caller]
    #[instrument(level = "debug", skip(holder))]
    pub fn new(holder: impl Into<String>) -> Self {
        let loc = Location::caller();
        Self {
            holder: holder.into(),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for StoreLockTimedOutSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "timed out waiting for store lock held by {} ({}:{})",
            self.holder, self.file, self.line
        )
    }
}

impl std::error::Error for StoreLockTimedOutSource {}

/// Umbrella kind for the advisory store-lock machinery, nested one
/// level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum StoreLockErrorKind {
    WaitingLockBusy(WaitingLockBusySource),
    WaitingSysrootLockBusy(WaitingSysrootLockBusySource),
    SharedLockExclusiveCommand(SharedLockExclusiveCommandSource),
    StoreLockTimedOut(StoreLockTimedOutSource),
}

#[derive(Debug)]
pub struct StoreLockError {
    kind: Box<StoreLockErrorKind>,
    file: String,
    line: u32,
}

impl StoreLockError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: StoreLockErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for StoreLockError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            StoreLockErrorKind::WaitingLockBusy(source) => source.fmt(formatter),
            StoreLockErrorKind::WaitingSysrootLockBusy(source) => source.fmt(formatter),
            StoreLockErrorKind::SharedLockExclusiveCommand(source) => source.fmt(formatter),
            StoreLockErrorKind::StoreLockTimedOut(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for StoreLockError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            StoreLockErrorKind::WaitingLockBusy(source) => Some(source),
            StoreLockErrorKind::WaitingSysrootLockBusy(source) => Some(source),
            StoreLockErrorKind::SharedLockExclusiveCommand(source) => Some(source),
            StoreLockErrorKind::StoreLockTimedOut(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// Waiting for the worklist lock returned busy.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn waiting_lock_busy() -> Self {
        Self::from_kind(super::CordialErrorKind::StoreLock(StoreLockError::new(
            StoreLockErrorKind::WaitingLockBusy(WaitingLockBusySource::new()),
        )))
    }

    /// Waiting for the sysroot lock returned busy.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn waiting_sysroot_lock_busy() -> Self {
        Self::from_kind(super::CordialErrorKind::StoreLock(StoreLockError::new(
            StoreLockErrorKind::WaitingSysrootLockBusy(WaitingSysrootLockBusySource::new()),
        )))
    }

    /// A shared lock cannot record an exclusive command.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn shared_lock_exclusive_command() -> Self {
        Self::from_kind(super::CordialErrorKind::StoreLock(StoreLockError::new(
            StoreLockErrorKind::SharedLockExclusiveCommand(SharedLockExclusiveCommandSource::new()),
        )))
    }

    /// Timed out waiting for a store lock.
    #[track_caller]
    #[instrument(level = "debug", skip(holder))]
    pub fn store_lock_timed_out(holder: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::StoreLock(StoreLockError::new(
            StoreLockErrorKind::StoreLockTimedOut(StoreLockTimedOutSource::new(holder)),
        )))
    }
}
