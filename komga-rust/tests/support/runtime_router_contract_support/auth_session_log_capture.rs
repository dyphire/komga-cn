use std::sync::{Arc, Mutex};

use komga_config::env_config::RuntimeConfig;

pub fn capture_router_logs_async_result<T, F>(config: &RuntimeConfig, action: F) -> (String, T)
where
    T: Send + 'static,
    F: std::future::Future<Output = T> + 'static,
{
    let result = Arc::new(Mutex::new(None::<T>));
    let result_slot = Arc::clone(&result);
    let logs = komga_server::logging::capture_for_test(config, move || {
        let output = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("router log capture runtime should build")
            .block_on(action);
        *result_slot
            .lock()
            .expect("router log capture result slot should not be poisoned") = Some(output);
    })
    .expect("router log capture should succeed");

    let output = result
        .lock()
        .expect("router log capture result slot should not be poisoned")
        .take()
        .expect("router log capture action should populate a result");

    (logs, output)
}

pub fn parse_json_log_lines(logs: &str) -> Vec<serde_json::Value> {
    logs.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("captured router log line should be valid JSON")
        })
        .collect()
}

pub fn matching_event_fields<'a>(
    events: &'a [serde_json::Value],
    event: &str,
) -> Vec<&'a serde_json::Map<String, serde_json::Value>> {
    events
        .iter()
        .filter_map(|entry| {
            let fields = entry.get("fields")?.as_object()?;
            (fields.get("event").and_then(serde_json::Value::as_str) == Some(event))
                .then_some(fields)
        })
        .collect()
}
