//! Where a command's output goes: the process streams, or a session client.
//!
//! Commands write through [`stdout`] and [`stderr`] instead of the process
//! streams. In an ordinary run those are the process streams. A session server
//! installs a sink for the thread that runs a request, and everything the
//! command writes there travels to the client that asked for it.

use std::cell::RefCell;
use std::io::{self, Write};
use std::sync::Arc;
use tracing::instrument;

/// Which of the two output streams a write is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// Receives the output of a request instead of the process streams.
pub trait FrameSink: Send + Sync {
    /// Deliver `bytes` written to `stream`.
    fn send(&self, stream: Stream, bytes: &[u8]) -> io::Result<()>;
}

thread_local! {
    static SINK: RefCell<Option<Arc<dyn FrameSink>>> = const { RefCell::new(None) };
}

/// Run `action` with this thread's output going to `sink`, restoring the
/// previous destination afterwards.
#[instrument(level = "debug", skip(sink, action))]
pub fn with_sink<T>(sink: Arc<dyn FrameSink>, action: impl FnOnce() -> T) -> T {
    let previous = SINK.with(|slot| slot.replace(Some(sink)));
    let result = action();
    SINK.with(|slot| *slot.borrow_mut() = previous);
    result
}

/// A writer for one of the output streams.
#[derive(Debug, Clone, Copy)]
pub struct Out {
    stream: Stream,
}

/// Standard output, or the current request's.
#[instrument(level = "trace")]
pub fn stdout() -> Out {
    Out {
        stream: Stream::Stdout,
    }
}

/// Standard error, or the current request's.
#[instrument(level = "trace")]
pub fn stderr() -> Out {
    Out {
        stream: Stream::Stderr,
    }
}

impl Write for Out {
    #[instrument(level = "trace", skip(self, buf))]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let sink = SINK.with(|slot| slot.borrow().clone());
        match (sink, self.stream) {
            (Some(sink), stream) => sink.send(stream, buf).map(|()| buf.len()),
            (None, Stream::Stdout) => io::stdout().write(buf),
            (None, Stream::Stderr) => io::stderr().write(buf),
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn flush(&mut self) -> io::Result<()> {
        let sink = SINK.with(|slot| slot.borrow().is_some());
        match (sink, self.stream) {
            (true, _) => Ok(()),
            (false, Stream::Stdout) => io::stdout().flush(),
            (false, Stream::Stderr) => io::stderr().flush(),
        }
    }
}
