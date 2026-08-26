//! Human-friendly terminal rendering with script-safe fallbacks.

use std::{
    io::{self, IsTerminal},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use comfy_table::{
    Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL_CONDENSED,
};
use indicatif::{ProgressBar, ProgressStyle};

static ACTIVE_WORKFLOW: OnceLock<Mutex<Option<ProgressBar>>> = OnceLock::new();

/// Whether standard output is an interactive terminal.
pub(crate) fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}

/// One consistent multi-phase progress bar shared by a complete workflow.
pub(crate) struct WorkflowProgress {
    bar: Option<ProgressBar>,
}

impl WorkflowProgress {
    /// Starts a workflow with a known number of high-level phases.
    pub(crate) fn new(label: &str, phases: u64) -> Self {
        let bar = io::stderr().is_terminal().then(|| {
            let bar = ProgressBar::new(phases);
            bar.set_style(
                ProgressStyle::with_template(
                    "{spinner:.cyan} {msg:<28} [{bar:28.cyan/blue}] {pos}/{len} {elapsed_precise}",
                )
                .expect("static workflow progress template is valid")
                .progress_chars("━━╸"),
            );
            bar.set_message(label.to_owned());
            bar.enable_steady_tick(Duration::from_millis(100));
            *ACTIVE_WORKFLOW
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("workflow progress lock is available") = Some(bar.clone());
            bar
        });
        if bar.is_none() {
            event("Sync", label);
        }
        Self { bar }
    }

    /// Updates the active phase label without advancing the bar.
    pub(crate) fn phase(&self, label: &str) {
        if let Some(bar) = &self.bar {
            bar.set_message(label.to_owned());
        } else {
            event("Sync", label);
        }
    }

    /// Marks one phase complete and displays its elapsed time.
    pub(crate) fn complete(&self, label: &str, elapsed: &str) {
        if let Some(bar) = &self.bar {
            bar.inc(1);
        }
        event("Done", format!("{label} · {elapsed}"));
    }

    /// Clears the workflow bar before the final report is rendered.
    pub(crate) fn finish(mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
        clear_active_workflow();
    }
}

impl Drop for WorkflowProgress {
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
        clear_active_workflow();
    }
}

fn clear_active_workflow() {
    *ACTIVE_WORKFLOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("workflow progress lock is available") = None;
}

/// Prints a consistently styled workflow event without disrupting an active bar.
pub(crate) fn event(scope: &str, message: impl AsRef<str>) {
    let line = if io::stderr().is_terminal() {
        format!(
            "\x1b[1;36m{scope}\x1b[0m \x1b[2m·\x1b[0m {}",
            message.as_ref()
        )
    } else {
        format!("{scope} · {}", message.as_ref())
    };
    if let Some(bar) = ACTIVE_WORKFLOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("workflow progress lock is available")
        .as_ref()
    {
        bar.println(line);
    } else {
        eprintln!("{line}");
    }
}

/// A colored progress bar on terminals and periodic plain lines elsewhere.
pub(crate) struct TerminalProgress {
    bar: Option<ProgressBar>,
    label: String,
    total: u64,
    last_plain: u64,
}

impl TerminalProgress {
    pub(crate) fn new(label: impl Into<String>, total: usize) -> Self {
        let label = label.into();
        let total = u64::try_from(total).unwrap_or(u64::MAX);
        let workflow_active = ACTIVE_WORKFLOW
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("workflow progress lock is available")
            .is_some();
        let bar = (io::stderr().is_terminal() && !workflow_active).then(|| {
            let bar = ProgressBar::new(total);
            let style = ProgressStyle::with_template(
                "{spinner:.cyan} {msg:<30} [{bar:32.cyan/blue}] {pos:>4}/{len:4} {elapsed_precise}",
            )
            .expect("static progress template is valid")
            .progress_chars("━━╸");
            bar.set_style(style);
            bar.set_message(label.clone());
            bar
        });
        Self {
            bar,
            label,
            total,
            last_plain: 0,
        }
    }

    pub(crate) fn set_position(&mut self, position: usize) {
        let position = u64::try_from(position).unwrap_or(self.total);
        if let Some(bar) = &self.bar {
            bar.set_position(position.min(self.total));
        } else if position == self.total || position.saturating_sub(self.last_plain) >= 50 {
            progress_event(&self.label, format!("{position}/{}", self.total));
            self.last_plain = position;
        }
    }

    pub(crate) fn note(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            bar.println(message.as_ref());
        } else {
            progress_event(&self.label, message.as_ref());
        }
    }

    pub(crate) fn finish(self) {
        if let Some(bar) = self.bar {
            bar.finish_and_clear();
        } else if self.last_plain != self.total {
            progress_event(&self.label, format!("{}/{}", self.total, self.total));
        }
    }
}

fn progress_event(label: &str, message: impl AsRef<str>) {
    if let Some((scope, activity)) = label.split_once(" · ") {
        event(scope, format!("{activity} · {}", message.as_ref()));
    } else {
        event(label, message);
    }
}

/// Renders a compact bordered table with a colored header.
pub(crate) fn pretty_table<S: AsRef<str>>(headers: &[S], rows: Vec<Vec<String>>) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(|header| {
            Cell::new(header.as_ref())
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold)
        }));
    for row in rows {
        table.add_row(row);
    }
    table.to_string()
}
