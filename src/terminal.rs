//! Human-friendly terminal rendering with script-safe fallbacks.

use std::io::{self, IsTerminal};

use comfy_table::{
    Attribute, Cell, Color, ContentArrangement, Table, presets::UTF8_FULL_CONDENSED,
};
use indicatif::{ProgressBar, ProgressStyle};

/// Whether standard output is an interactive terminal.
pub(crate) fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
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
        let bar = io::stderr().is_terminal().then(|| {
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
            eprintln!("{} {position}/{}", self.label, self.total);
            self.last_plain = position;
        }
    }

    pub(crate) fn note(&self, message: impl AsRef<str>) {
        if let Some(bar) = &self.bar {
            bar.println(message.as_ref());
        } else {
            eprintln!("{}", message.as_ref());
        }
    }

    pub(crate) fn finish(self) {
        if let Some(bar) = self.bar {
            bar.finish_and_clear();
        } else if self.last_plain != self.total {
            eprintln!("{} {}/{}", self.label, self.total, self.total);
        }
    }
}

/// Renders a compact bordered table with a colored header.
pub(crate) fn pretty_table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(|header| {
            Cell::new(header)
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold)
        }));
    for row in rows {
        table.add_row(row);
    }
    table.to_string()
}
