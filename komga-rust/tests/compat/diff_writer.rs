use crate::compat::{
    ComparisonMode, DiffReport, NormalizedBody, NormalizedResponse, SerializableResponse,
};
use anyhow::Context;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn compare_responses(
    case_id: &str,
    java: &NormalizedResponse,
    rust: &NormalizedResponse,
    allowlist: &BTreeSet<String>,
    comparison_mode: ComparisonMode,
) -> DiffReport {
    let mut differences = Vec::new();

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

    if comparison_mode == ComparisonMode::Json && java.body != rust.body {
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

pub fn write_diff_report(output_dir: &Path, report: &DiffReport) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diff output dir {}", output_dir.display()))?;
    let report_path = output_dir.join(format!("{}.json", report.case_id));
    let report_json =
        serde_json::to_string_pretty(report).context("failed to serialize diff report")?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed to write diff report {}", report_path.display()))
}

fn render_body(body: &NormalizedBody) -> String {
    match body {
        NormalizedBody::Json(value) => value.to_string(),
        NormalizedBody::Text(value) => value.clone(),
        NormalizedBody::Empty => "<empty>".to_string(),
    }
}
