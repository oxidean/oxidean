//! Spinner — `indicatif` progress on stderr-tty, silent no-op when piped.
//!
//! Wrap around long waits (device-grant polling, `run watch`, release asset
//! uploads). Never writes to stdout; disappears entirely on non-TTY so piped
//! output stays byte-clean.

use indicatif::{ProgressBar, ProgressStyle};

pub struct Spinner {
    bar: Option<ProgressBar>,
}

impl Spinner {
    /// Spinning indicator with `msg` on stderr — `None` bar when not a TTY.
    pub fn new(msg: &str) -> Self {
        if !crate::term::stderr_tty() {
            return Self { bar: None };
        }
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );
        bar.set_message(msg.to_string());
        bar.enable_steady_tick(std::time::Duration::from_millis(80));
        Self { bar: Some(bar) }
    }

    /// Byte-progress bar (release asset upload). `total` in bytes.
    pub fn progress(total: u64, msg: &str) -> Self {
        if !crate::term::stderr_tty() {
            return Self { bar: None };
        }
        let bar = ProgressBar::new(total);
        bar.set_style(
            ProgressStyle::with_template("{msg} {bar:30.cyan/blue} {bytes}/{total_bytes} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar()),
        );
        bar.set_message(msg.to_string());
        Self { bar: Some(bar) }
    }

    pub fn set_message(&self, msg: &str) {
        if let Some(b) = &self.bar {
            b.set_message(msg.to_string());
        }
    }

    pub fn inc(&self, n: u64) {
        if let Some(b) = &self.bar {
            b.inc(n);
        }
    }

    /// Stop and clear — call before printing results so the bar doesn't
    /// corrupt text output.
    pub fn finish(&self) {
        if let Some(b) = &self.bar {
            b.finish_and_clear();
        }
    }

    /// Stop and leave `msg` on stderr (kept for "waiting… done" flows).
    pub fn finish_with(&self, msg: &str) {
        if let Some(b) = &self.bar {
            b.finish_with_message(msg.to_string());
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Some(b) = &self.bar {
            b.finish_and_clear();
        }
    }
}
