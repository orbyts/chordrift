//! Shared interactive rendering for every command-line report.

use serde_json::Value;

use crate::terminal;

/// Converts stable plain-text command output into a human-friendly terminal report.
///
/// Redirected output never passes through this renderer, preserving the existing
/// script interface. Bespoke interactive reports containing ANSI or box-drawing
/// output are already rendered and pass through unchanged.
pub(crate) fn render_interactive(raw: &str) -> String {
    if raw.contains("\x1b[")
        || raw
            .lines()
            .any(|line| line.starts_with(['┌', '╞', '│', '└']))
    {
        return raw.to_owned();
    }

    let lines = raw.lines().collect::<Vec<_>>();
    let mut rendered = String::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].is_empty() {
            if !rendered.ends_with("\n\n") {
                rendered.push('\n');
            }
            index += 1;
            continue;
        }
        if lines[index].contains('\t') {
            let start = index;
            while index < lines.len() && !lines[index].is_empty() && lines[index].contains('\t') {
                index += 1;
            }
            push_tabular(&mut rendered, &lines[start..index]);
            continue;
        }
        if split_field(lines[index]).is_some() {
            let start = index;
            while index < lines.len()
                && !lines[index].is_empty()
                && !lines[index].contains('\t')
                && split_field(lines[index]).is_some()
            {
                index += 1;
            }
            push_fields(&mut rendered, &lines[start..index]);
            continue;
        }

        rendered.push_str(lines[index]);
        rendered.push('\n');
        index += 1;
    }
    rendered
}

fn split_field(line: &str) -> Option<(&str, &str)> {
    line.split_once(": ")
}

fn push_fields(rendered: &mut String, lines: &[&str]) {
    let fields = lines
        .iter()
        .filter_map(|line| split_field(line))
        .collect::<Vec<_>>();
    let Some((headline_key, headline_value)) = fields.first().copied() else {
        return;
    };
    if !rendered.is_empty() && !rendered.ends_with("\n\n") {
        rendered.push('\n');
    }
    rendered.push_str("\x1b[1;36m");
    rendered.push_str(&humanize(headline_key));
    rendered.push_str("\x1b[0m  \x1b[2m— ");
    rendered.push_str(headline_value);
    rendered.push_str("\x1b[0m\n");
    if fields.len() > 1 {
        rendered.push_str(&terminal::pretty_table(
            &["Field", "Value"],
            fields[1..]
                .iter()
                .map(|(key, value)| vec![humanize(key), human_value(value)])
                .collect(),
        ));
        rendered.push('\n');
    }
}

fn push_tabular(rendered: &mut String, lines: &[&str]) {
    let Some(header_line) = lines.first() else {
        return;
    };
    let headers = header_line.split('\t').collect::<Vec<_>>();
    let rows = lines[1..]
        .iter()
        .map(|line| {
            line.split('\t')
                .enumerate()
                .map(|(index, value)| {
                    if matches!(
                        headers.get(index),
                        Some(&"payload" | &"safety" | &"evidence")
                    ) {
                        summarize_json(value)
                    } else {
                        human_value(value)
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if !rendered.is_empty() && !rendered.ends_with("\n\n") {
        rendered.push('\n');
    }
    rendered.push_str(&terminal::pretty_table(
        &headers
            .iter()
            .map(|header| humanize(header))
            .collect::<Vec<_>>(),
        rows,
    ));
    rendered.push('\n');
}

fn humanize(value: &str) -> String {
    let mut value = value.replace('_', " ");
    if let Some(first) = value.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    value
}

fn human_value(value: &str) -> String {
    match value {
        "true" => "yes".to_owned(),
        "false" => "no".to_owned(),
        _ => value.to_owned(),
    }
}

fn summarize_json(raw: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return raw.to_owned();
    };
    let mut parts = Vec::new();
    flatten_json(None, &value, &mut parts);
    if parts.is_empty() {
        "—".to_owned()
    } else {
        parts.join(" · ")
    }
}

fn flatten_json(prefix: Option<&str>, value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let name = match prefix {
                    Some(prefix) => format!("{prefix} {key}"),
                    None => key.clone(),
                };
                flatten_json(Some(&name), value, parts);
            }
        }
        Value::Array(values) => parts.push(format!(
            "{}={}",
            prefix.map_or_else(|| "items".to_owned(), humanize),
            values.len()
        )),
        Value::Bool(false) | Value::Null => {}
        Value::Bool(true) => parts.push(prefix.map_or_else(|| "yes".to_owned(), humanize)),
        Value::String(value) => parts.push(format!(
            "{}={value}",
            prefix.map_or_else(|| "value".to_owned(), humanize)
        )),
        value => parts.push(format!(
            "{}={value}",
            prefix.map_or_else(|| "value".to_owned(), humanize)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::render_interactive;

    #[test]
    fn renders_fields_and_tabular_json_as_one_terminal_report() {
        let rendered = render_interactive(
            "sync_plan: already current\noperations: 1\nspotify_writes: disabled\n\
             sequence\tphase\toperation\tpayload\tsafety\n\
             0\treconcile\texclude_track\t{\"reason\":\"removed\"}\t{\"neon_only\":true,\"destructive\":false}\n",
        );
        assert!(rendered.contains("Sync plan"));
        assert!(rendered.contains("Operations"));
        assert!(rendered.contains("Reason=removed"));
        assert!(rendered.contains("Neon only"));
        assert!(!rendered.contains("{\"reason\""));
    }

    #[test]
    fn preserves_bespoke_interactive_output() {
        let report = "\x1b[1mCurrent library\x1b[0m\n┌──┐\n";
        assert_eq!(render_interactive(report), report);
    }
}
