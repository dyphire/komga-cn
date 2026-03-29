use super::config::SseAudience;
use super::parser::NormalizedEventLog;
use super::report::{SerializableEventLog, SseDiffReport};

pub fn compare_event_logs(
    case_id: &str,
    audience: SseAudience,
    java: &NormalizedEventLog,
    rust: &NormalizedEventLog,
) -> SseDiffReport {
    let mut differences = Vec::new();
    let audience_label = audience_label(audience);

    if java.events.len() != rust.events.len() {
        differences.push(format!(
            "filtered event count mismatch for {audience_label} audience: java={} rust={}",
            java.events.len(),
            rust.events.len()
        ));
    }

    for (index, (java_event, rust_event)) in java.events.iter().zip(rust.events.iter()).enumerate()
    {
        if java_event.name != rust_event.name {
            differences.push(format!(
                "event[{index}] name mismatch: java={} rust={}",
                java_event.name, rust_event.name
            ));
        }

        if java_event.payload != rust_event.payload {
            differences.push(format!(
                "event[{index}] payload mismatch: java={} rust={}",
                render_payload(&java_event.payload),
                render_payload(&rust_event.payload)
            ));
        }
    }

    if java.events.len() > rust.events.len() {
        for (index, event) in java.events.iter().enumerate().skip(rust.events.len()) {
            differences.push(format!(
                "event[{index}] present only in java: name={} payload={}",
                event.name,
                render_payload(&event.payload)
            ));
        }
    }

    if rust.events.len() > java.events.len() {
        for (index, event) in rust.events.iter().enumerate().skip(java.events.len()) {
            differences.push(format!(
                "event[{index}] present only in rust: name={} payload={}",
                event.name,
                render_payload(&event.payload)
            ));
        }
    }

    SseDiffReport {
        case_id: case_id.to_string(),
        audience,
        matches: differences.is_empty(),
        differences,
        java: SerializableEventLog::from(java),
        rust: SerializableEventLog::from(rust),
    }
}

fn render_payload(payload: &serde_json::Value) -> String {
    payload.to_string()
}

fn audience_label(audience: SseAudience) -> &'static str {
    match audience {
        SseAudience::Any => "any",
        SseAudience::Admin => "admin",
        SseAudience::User => "user",
    }
}
