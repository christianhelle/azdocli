//! Progress bar helpers built on `indicatif`. Falls back to plain logging
//! when stderr is not a TTY.

#![allow(dead_code)]

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct ProgressReporter {
    multi: MultiProgress,
}

impl ProgressReporter {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
        }
    }

    pub fn phase_bar(&self, name: &str, total: u64) -> ProgressBar {
        let bar = self.multi.add(ProgressBar::new(total.max(1)));
        let style = ProgressStyle::with_template(
            "{prefix:>22} [{bar:30.cyan/blue}] {pos:>5}/{len:5} {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("##-");
        bar.set_style(style);
        bar.set_prefix(name.to_string());
        bar
    }

    pub fn println(&self, msg: impl AsRef<str>) {
        let _ = self.multi.println(msg.as_ref());
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}
