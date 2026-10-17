use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cordial::{LineProgress, ProgressSink};
use miette::IntoDiagnostic;

#[derive(Clone, Default)]
struct CapturedLines(Arc<Mutex<Vec<u8>>>);

impl CapturedLines {
    fn text(&self) -> miette::Result<String> {
        let bytes = self
            .0
            .lock()
            .map_err(|error| miette::miette!("capture lock: {error}"))?;
        String::from_utf8(bytes.clone()).into_diagnostic()
    }
}

impl Write for CapturedLines {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("capture lock poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn line_progress_reports_start_heartbeat_and_finish() -> miette::Result<()> {
    cordial::init_tracing();
    let capture = CapturedLines::default();
    let progress = LineProgress::with_writer(capture.clone(), Duration::from_millis(20));
    let task = progress.bar("Loading".to_string(), 4);
    task.inc(2);
    task.set_message("Loading crates".to_string());
    std::thread::sleep(Duration::from_millis(75));
    task.finish("Loaded 4 crates".to_string());

    let lines = capture.text()?;
    assert!(lines.starts_with("Starting: Loading\n"), "{lines}");
    assert!(
        lines.contains("still working: Loading crates 2/4"),
        "{lines}"
    );
    assert!(lines.ends_with("Finished: Loaded 4 crates\n"), "{lines}");
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(lines, capture.text()?, "heartbeat continued after finish");
    Ok(())
}
