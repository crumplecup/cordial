//! Exception registry loading, matching, and persistence.

mod add;
mod backup;
mod entry;
mod json;
mod load;
mod paths;

pub use add::{AddExceptionOutcome, CoverageSkipEntry, add_coverage_skip, add_exception};
pub use backup::{backup_exception_files, load_exception_files};
pub use entry::{ExceptionEntry, ExceptionSet, apply_exception_sets};
pub use load::{load_exception_sets, load_exceptions};
pub use paths::{
    DEFAULT_EXCEPTIONS_REGISTRY, coverage_skip_file_path, exception_file_path,
    resolve_exceptions_root,
};
