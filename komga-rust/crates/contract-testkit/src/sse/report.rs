use super::config::SseAudience;
use super::parser::{NormalizedEvent, NormalizedEventLog};
use anyhow::Context;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct SseDiffReport {
    pub case_id: String,
    pub audience: SseAudience,
    pub matches: bool,
    pub differences: Vec<String>,
    pub java: SerializableEventLog,
    pub rust: SerializableEventLog,
}

#[derive(Debug, Clone, Serialize)]
pub struct SerializableEventLog {
    pub events: Vec<SerializableEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SerializableEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

impl From<&NormalizedEventLog> for SerializableEventLog {
    fn from(value: &NormalizedEventLog) -> Self {
        Self {
            events: value.events.iter().map(SerializableEvent::from).collect(),
        }
    }
}

impl From<&NormalizedEvent> for SerializableEvent {
    fn from(value: &NormalizedEvent) -> Self {
        Self {
            name: value.name.clone(),
            payload: value.payload.clone(),
        }
    }
}

pub fn write_diff_report(output_dir: &Path, report: &SseDiffReport) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diff output dir {}", output_dir.display()))?;
    let report_path = output_dir.join(format!("{}.json", report.case_id));
    let report_json =
        serde_json::to_string_pretty(report).context("failed to serialize diff report")?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed to write diff report {}", report_path.display()))
}
