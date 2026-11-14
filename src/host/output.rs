//! The process's output streams, as commands write to them.
//!
//! Commands write through [`stdout`] and [`stderr`] rather than the `std`
//! functions, so that where output goes is decided in one place.

use std::io::{self, Write};
use tracing::instrument;

/// Which of the two output streams a writer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
}

/// A writer for one of the process's output streams.
#[derive(Debug, Clone, Copy)]
pub struct Out {
    stream: Stream,
}

/// Standard output.
#[instrument(level = "trace")]
pub fn stdout() -> Out {
    Out {
        stream: Stream::Stdout,
    }
}

/// Standard error.
#[instrument(level = "trace")]
pub fn stderr() -> Out {
    Out {
        stream: Stream::Stderr,
    }
}

impl Write for Out {
    #[instrument(level = "trace", skip(self, buf))]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.stream {
            Stream::Stdout => io::stdout().write(buf),
            Stream::Stderr => io::stderr().write(buf),
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn flush(&mut self) -> io::Result<()> {
        match self.stream {
            Stream::Stdout => io::stdout().flush(),
            Stream::Stderr => io::stderr().flush(),
        }
    }
}
