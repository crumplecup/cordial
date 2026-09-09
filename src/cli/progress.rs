//! Indicatif-backed CLI progress presentation.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::{ProgressSink, ProgressTask};

#[derive(Debug, Clone)]
pub(super) struct IndicatifProgress {
    multi: MultiProgress,
}

impl IndicatifProgress {
    pub(super) fn new() -> Self {
        let draw_target = if std::env::var_os("CORDIAL_NO_PROGRESS").is_some()
            || !std::io::stderr().is_terminal()
        {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        };
        Self {
            multi: MultiProgress::with_draw_target(draw_target),
        }
    }
}

impl ProgressSink for IndicatifProgress {
    fn spinner(&self, message: String) -> Box<dyn ProgressTask> {
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(spinner_style());
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(100));
        Box::new(IndicatifTask::new(bar))
    }

    fn bar(&self, message: String, len: u64) -> Box<dyn ProgressTask> {
        let bar = self.multi.add(ProgressBar::new(len));
        bar.set_style(bar_style());
        bar.set_message(message);
        Box::new(IndicatifTask::new(bar))
    }

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
    fn new(bar: ProgressBar) -> Self {
        Self {
            bar,
            finished: AtomicBool::new(false),
        }
    }
}

impl ProgressTask for IndicatifTask {
    fn set_message(&self, message: String) {
        self.bar.set_message(message);
    }

    fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }

    fn finish(&self, message: String) {
        self.finished.store(true, Ordering::Relaxed);
        self.bar.finish_with_message(message);
    }
}

impl Drop for IndicatifTask {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::Relaxed) {
            self.bar.abandon();
        }
    }
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
        .tick_strings(&["-", "\\", "|", "/"])
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{wide_bar:.cyan/blue} {pos}/{len} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-")
}
