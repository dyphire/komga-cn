use super::normalize::comparable_body;
use crate::{ComparisonMode, DiffReport, NormalizedBody, NormalizedResponse, SerializableResponse};
use std::collections::BTreeSet;

pub fn compare_responses(
    case_id: &str,
    java: &NormalizedResponse,
    rust: &NormalizedResponse,
    allowlist: &BTreeSet<String>,
    comparison_mode: ComparisonMode,
) -> DiffReport {
    let mut differences = Vec::new();
    let comparable_java_body = comparable_body(case_id, &java.body);
    let comparable_rust_body = comparable_body(case_id, &rust.body);

    if java.status != rust.status {
        differences.push(format!(
            "status mismatch: java={} rust={}",
            java.status, rust.status
        ));
    }

    for header in allowlist {
        let java_value = java.headers.get(header);
        let rust_value = rust.headers.get(header);
        if java_value != rust_value {
            differences.push(format!(
                "header mismatch for {header}: java={java_value:?} rust={rust_value:?}"
            ));
        }
    }

    if matches!(comparison_mode, ComparisonMode::Json | ComparisonMode::Xml)
        && comparable_java_body != comparable_rust_body
    {
        differences.push(format!(
            "body mismatch: java={} rust={}",
            render_body(&java.body),
            render_body(&rust.body)
        ));
    }

    DiffReport {
        case_id: case_id.to_string(),
        matches: differences.is_empty(),
        differences,
        java: SerializableResponse::from(java),
        rust: SerializableResponse::from(rust),
    }
}

fn render_body(body: &NormalizedBody) -> String {
    match body {
        NormalizedBody::Json(value) => value.to_string(),
        NormalizedBody::Text(value) => value.clone(),
        NormalizedBody::Empty => "<empty>".to_string(),
    }
}
