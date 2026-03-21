use crate::cases::SetupStep;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct SseHarnessConfig {
    pub output_dir: String,
    pub cases: Vec<SseCaseConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SseCaseConfig {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub audience: SseAudience,
    pub headers: Option<BTreeMap<String, String>>,
    #[allow(dead_code)]
    pub setup: Option<Vec<SetupStep>>,
    #[serde(default = "default_ignore_heartbeats")]
    pub ignore_heartbeats: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SseAudience {
    #[default]
    Any,
    Admin,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedEventLog {
    pub events: Vec<NormalizedEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SseParseOptions {
    pub ignore_comments: bool,
    pub ignore_heartbeats: bool,
}

impl Default for SseParseOptions {
    fn default() -> Self {
        Self {
            ignore_comments: true,
            ignore_heartbeats: true,
        }
    }
}

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

impl SseHarnessConfig {
    pub fn load_default() -> anyhow::Result<Self> {
        Self::load_from(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/compat/sse-cases.toml"),
        )
    }

    pub fn load_from(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read SSE compat cases from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse SSE compat cases from {}", path.display()))
    }
}

impl SseCaseConfig {
    pub fn header_allowlist(&self) -> BTreeSet<String> {
        let mut allowlist = BTreeSet::new();
        allowlist.insert("content-type".to_string());

        if let Some(headers) = &self.headers {
            for header in headers.keys() {
                allowlist.insert(header.to_ascii_lowercase());
            }
        }

        allowlist
    }
}

pub fn parse_event_log(input: &str) -> anyhow::Result<NormalizedEventLog> {
    parse_event_log_with_options(input, &SseParseOptions::default())
}

pub fn parse_event_log_with_options(
    input: &str,
    options: &SseParseOptions,
) -> anyhow::Result<NormalizedEventLog> {
    let mut events = Vec::new();
    let mut frame = SseFrame::default();

    for raw_line in input.lines() {
        let line = raw_line.trim_end_matches('\r');

        if line.is_empty() {
            if !frame.is_empty() {
                if let Some(event) = frame.finish(options)? {
                    events.push(event);
                }
            }
            continue;
        }

        if line.starts_with(':') {
            frame.skipped = true;
            continue;
        }

        if let Some(value) = line.strip_prefix("event:") {
            frame.event_name = Some(value.trim_start().to_string());
            continue;
        }

        if let Some(value) = line.strip_prefix("data:") {
            frame.data_lines.push(value.trim_start().to_string());
            continue;
        }

        if line.starts_with("id:") || line.starts_with("retry:") {
            continue;
        }
    }

    if !frame.is_empty() {
        if let Some(event) = frame.finish(options)? {
            events.push(event);
        }
    }

    Ok(NormalizedEventLog { events })
}

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

pub fn write_diff_report(output_dir: &Path, report: &SseDiffReport) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diff output dir {}", output_dir.display()))?;
    let report_path = output_dir.join(format!("{}.json", report.case_id));
    let report_json =
        serde_json::to_string_pretty(report).context("failed to serialize diff report")?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed to write diff report {}", report_path.display()))
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

fn default_ignore_heartbeats() -> bool {
    true
}

#[derive(Debug, Default)]
struct SseFrame {
    event_name: Option<String>,
    data_lines: Vec<String>,
    skipped: bool,
}

impl SseFrame {
    fn is_empty(&self) -> bool {
        self.event_name.is_none() && self.data_lines.is_empty() && !self.skipped
    }

    fn finish(&mut self, options: &SseParseOptions) -> anyhow::Result<Option<NormalizedEvent>> {
        if self.skipped && options.ignore_comments {
            self.clear();
            return Ok(None);
        }

        let event_name = self
            .event_name
            .take()
            .unwrap_or_else(|| "message".to_string());
        let data = self.data_lines.join("\n");
        let skipped_heartbeat = options.ignore_heartbeats && is_heartbeat(&event_name);
        self.clear();

        if skipped_heartbeat {
            return Ok(None);
        }

        if data.is_empty() {
            return Ok(Some(NormalizedEvent {
                name: event_name,
                payload: serde_json::Value::Null,
            }));
        }

        let payload = match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(value) => normalize_json_value(value),
            Err(_) => serde_json::Value::String(data),
        };

        Ok(Some(NormalizedEvent {
            name: event_name,
            payload,
        }))
    }

    fn clear(&mut self) {
        self.event_name = None;
        self.data_lines.clear();
        self.skipped = false;
    }
}

fn is_heartbeat(event_name: &str) -> bool {
    matches!(event_name, "heartbeat" | "keepalive" | "ping")
}

fn normalize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(normalize_json_value).collect())
        }
        serde_json::Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, normalize_json_value(value)))
                .collect::<serde_json::Map<String, serde_json::Value>>();
            serde_json::Value::Object(sorted)
        }
        primitive => primitive,
    }
}
