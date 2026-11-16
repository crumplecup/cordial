//! The log can show each span when it ends, with its time, for the modules a run asks about.

use cordial::{LOG_SPANS_VARIABLE, span_events};
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;

#[test]
fn the_setting_chooses_which_span_events_the_log_shows() {
    cordial::init_tracing();
    assert_eq!(span_events(None), FmtSpan::NONE);
    assert_eq!(span_events(Some("")), FmtSpan::NONE);
    assert_eq!(span_events(Some("close")), FmtSpan::CLOSE);
    assert_eq!(span_events(Some("all")), FmtSpan::FULL);
    assert_eq!(LOG_SPANS_VARIABLE, "CORDIAL_LOG_SPANS");
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut captured) = self.0.lock() {
            captured.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Captured;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tracing::instrument(level = "debug")]
fn some_work() {
    std::thread::sleep(std::time::Duration::from_millis(5));
}

#[test]
fn a_span_that_ends_says_how_long_it_ran_when_the_setting_asks() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_span_events(span_events(Some("close")))
        .finish();
    tracing::subscriber::with_default(subscriber, some_work);
    let text = String::from_utf8(
        captured
            .0
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default(),
    )
    .unwrap_or_default();
    assert!(text.contains("some_work"), "{text}");
    assert!(text.contains("time.busy"), "{text}");
}
