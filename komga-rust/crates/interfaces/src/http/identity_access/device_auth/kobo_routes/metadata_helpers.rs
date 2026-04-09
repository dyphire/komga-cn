use super::*;

pub(super) fn kobo_description(summary: &str) -> Value {
    if summary.trim().is_empty() {
        Value::String(" ".to_string())
    } else {
        Value::String(summary.to_string())
    }
}

pub(super) fn kobo_language(language: &str) -> String {
    let language = language.trim();
    if language.is_empty() {
        "en".to_string()
    } else {
        language
            .chars()
            .take(2)
            .collect::<String>()
            .to_ascii_lowercase()
    }
}

pub(super) fn kobo_publication_date_value(value: &str) -> Option<Value> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
        Some(Value::String(format!("{value}T00:00:00Z")))
    } else {
        Some(Value::String(value.to_string()))
    }
}
