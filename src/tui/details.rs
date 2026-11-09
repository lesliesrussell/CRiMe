use super::theme::Palette;
use crate::*;
use ratatui::{
    style::Modifier,
    text::{Line, Span, Text},
};

pub fn label(key: &str) -> String {
    key.trim_end_matches("_id")
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|first| first.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn scalar(value: &Value, records: &[Value]) -> String {
    match value {
        Value::String(s) => records
            .iter()
            .find(|r| r["id"].as_str() == Some(s))
            .and_then(|r| r["name"].as_str())
            .unwrap_or(s)
            .to_owned(),
        Value::Null => "—".into(),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.into(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => a
            .iter()
            .map(|v| scalar(v, records))
            .collect::<Vec<_>>()
            .join(" · "),
        Value::Object(_) => "".into(),
    }
}
fn field(
    lines: &mut Vec<Line<'static>>,
    key: &str,
    value: &Value,
    depth: usize,
    records: &[Value],
    p: Palette,
) {
    let indent = "  ".repeat(depth);
    lines.push(Line::from(Span::styled(
        format!("{indent}{}", label(key)),
        p.base().fg(p.muted),
    )));
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                field(lines, k, v, depth + 1, records, p)
            }
        }
        Value::Array(a) if a.iter().any(|v| v.is_object() || v.is_array()) => {
            for (i, v) in a.iter().enumerate() {
                field(lines, &format!("Item {}", i + 1), v, depth + 1, records, p)
            }
        }
        _ => {
            let text = scalar(value, records);
            for line in text.lines() {
                lines.push(Line::from(format!("{indent}  {line}")));
            }
            if text.is_empty() {
                lines.push(Line::from(format!("{indent}  —")));
            }
        }
    }
    if depth == 0 {
        lines.push(Line::from(""));
    }
}
pub fn record(record: &Value, records: &[Value], p: Palette) -> Text<'static> {
    let mut lines = vec![
        Line::from(Span::styled(
            record["name"].as_str().unwrap_or("Untitled").to_owned(),
            p.base().fg(p.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(label(record["kind"].as_str().unwrap_or("record"))),
        Line::from(""),
    ];
    if let Some(data) = record["data"].as_object() {
        for (key, value) in data {
            field(&mut lines, key, value, 0, records, p);
        }
    }
    if record["data"].as_object().is_some_and(|m| m.is_empty()) {
        lines.push(Line::from("No additional details yet."));
        lines.push(Line::from("Press e to add details in the edit form."));
        lines.push(Line::from(""));
    }
    for key in ["created", "updated", "id"] {
        if let Some(value) = record.get(key) {
            field(&mut lines, key, value, 0, &[], p);
        }
    }
    Text::from(lines)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_friendly_fields_links_and_nested_data() {
        let company = json!({"id":"company-1","name":"Engine Works"});
        let person = json!({"name":"Ada","kind":"contact","data":{"company_id":"company-1","tags":["VIP","customer"],"active":true,"address":{"city":"London"},"notes":"First line\nSecond line"}});
        let text = record(
            &person,
            &[company],
            super::super::theme::Theme::Dracula.palette(),
        )
        .to_string();
        assert!(text.contains("Company\n  Engine Works"));
        assert!(text.contains("VIP · customer"));
        assert!(text.contains("Yes"));
        assert!(text.contains("City"));
        assert!(text.contains("Second line"));
        assert!(!text.contains("company_id"));
        assert!(!text.contains('{'));
    }
}
