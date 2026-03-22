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
