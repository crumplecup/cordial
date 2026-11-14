//! Indicatif-backed CLI progress presentation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::{ProgressSink, ProgressTask};

use tracing::instrument;
#[derive(Debug, Clone)]
pub(super) struct IndicatifProgress {
    multi: MultiProgress,
}

impl IndicatifProgress {
    #[instrument(level = "debug")]
    #[hotpath::measure]
    pub(super) fn new() -> Self {
        Self {
            multi: MultiProgress::with_draw_target(ProgressDrawTarget::stderr()),
        }
    }

    #[instrument(level = "debug")]
    #[hotpath::measure]
    pub(super) fn hidden() -> Self {
        Self {
            multi: MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
        }
    }
}

impl ProgressSink for IndicatifProgress {
    #[instrument(level = "trace", skip(self))]
    fn spinner(&self, message: String) -> Box<dyn ProgressTask> {
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(spinner_style());
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(100));
        Box::new(IndicatifTask::new(bar))
    }

    #[instrument(level = "trace", skip(self))]
    fn bar(&self, message: String, len: u64) -> Box<dyn ProgressTask> {
        let bar = self.multi.add(ProgressBar::new(len));
        bar.set_style(bar_style());
        bar.set_message(message);
        // The live status moves while the position does not.
        bar.enable_steady_tick(Duration::from_millis(500));
        Box::new(IndicatifTask::new(bar))
    }

    #[instrument(level = "trace", skip(self))]
    fn notice(&self, message: String) {
        let _ = self.multi.println(message);
    }
}

#[derive(Debug)]
struct IndicatifTask {
    bar: ProgressBar,
    finished: AtomicBool,
}

impl IndicatifTask {
    #[instrument(level = "debug", skip(bar))]
    #[hotpath::measure]
    fn new(bar: ProgressBar) -> Self {
        Self {
            bar,
            finished: AtomicBool::new(false),
        }
    }
}

impl ProgressTask for IndicatifTask {
    #[instrument(level = "trace", skip(self))]
    fn set_message(&self, message: String) {
        self.bar.set_message(message);
    }

    #[instrument(level = "trace", skip(self))]
    fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }

    #[instrument(level = "trace", skip(self))]
    fn finish(&self, message: String) {
        self.finished.store(true, Ordering::Relaxed);
        self.bar.finish_with_message(message);
    }
}

impl Drop for IndicatifTask {
    #[instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        if !self.finished.load(Ordering::Relaxed) {
            self.bar.abandon();
        }
    }
}

#[instrument(level = "debug")]
#[hotpath::measure]
fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg} {live}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
        .with_key("live", live)
        .tick_strings(&["-", "\\", "|", "/"])
}

#[instrument(level = "debug")]
#[hotpath::measure]
fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{wide_bar:.cyan/blue} {pos}/{len} {msg} {live}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .with_key("live", live)
        .progress_chars("=>-")
}

/// What the run is doing, drawn with every refresh.
fn live(_: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write) {
    let _ = write!(writer, "[{}]", crate::LiveStatus::global().line());
}
