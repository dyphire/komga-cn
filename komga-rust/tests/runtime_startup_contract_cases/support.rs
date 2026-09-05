use super::*;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub(super) const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

static STARTUP_CONTRACT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) fn startup_contract_lock() -> MutexGuard<'static, ()> {
    STARTUP_CONTRACT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("startup contract lock should not be poisoned")
}

pub(super) async fn connect_test_pool(
    path: impl AsRef<std::path::Path>,
    max_connections: u32,
) -> Result<sqlx::SqlitePool, sqlx::Error> {
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(komga_infrastructure_base::file_backed_connect_options(path))
        .await
}

pub(super) fn unique_temp_dir(prefix: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{millis}"))
}

pub(super) fn create_stale_schema_search_index(index_dir: &std::path::Path) {
    fs::create_dir_all(index_dir).expect("stale schema index directory should be created");

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("doc_key", STRING | STORED);
    schema_builder.add_text_field("entity_id", STRING | STORED);
    let stale_schema = schema_builder.build();

    Index::create_in_dir(index_dir, stale_schema)
        .expect("stale schema runtime index should be created");
}

pub(super) fn create_runtime_index_with_stale_analyzer_version(index_dir: &std::path::Path) {
    komga_infrastructure_search::SearchIndexLifecycle::bootstrap(index_dir)
        .expect("runtime index fixture should bootstrap");
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        stale_analyzer_version().to_string(),
    )
    .expect("stale analyzer version marker should be written");
}

pub(super) fn stale_analyzer_version() -> u32 {
    search_analyzer_version().saturating_add(1)
}

pub(super) fn runtime_config_for_logging_contract(
    prefix: &str,
) -> komga_config::env_config::RuntimeConfig {
    let root = unique_temp_dir(prefix);
    fs::create_dir_all(&root).expect("logging contract temp root should be created");

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
    );
    config.config_dir = Some(root.clone());
    config.log_file = root.join("logs").join("komga.log");
    config.database_file = root.join("database.sqlite");
    config.riir_db_file = root.join("riir.sqlite");
    config.tasks_db_file = root.join("tasks.sqlite");
    config.lucene_data_directory = root.join("lucene");
    config.fonts_data_directory = root.join("fonts");
    config
}

pub(super) fn capture_contract_log_async<F>(
    config: &komga_config::env_config::RuntimeConfig,
    action: F,
) -> String
where
    F: std::future::Future<Output = ()> + 'static,
{
    komga_server::logging::capture_for_test(config, move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(action);
    })
    .expect("async test-local logging capture should succeed")
}

pub(super) fn capture_contract_log_async_result<T, F>(
    config: &komga_config::env_config::RuntimeConfig,
    action: F,
) -> (String, T)
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
            .expect("test runtime should build")
            .block_on(action);
        *result_slot
            .lock()
            .expect("async result slot should not be poisoned") = Some(output);
    })
    .expect("async test-local logging capture with result should succeed");

    let output = result
        .lock()
        .expect("async result slot should not be poisoned")
        .take()
        .expect("async logging action should populate a result");

    (logs, output)
}

pub(super) fn parse_json_log_lines(logs: &str) -> Vec<serde_json::Value> {
    logs.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("captured runtime log line should be valid JSON")
        })
        .collect()
}

pub(super) fn event_fields<'a>(
    events: &'a [serde_json::Value],
    event: &str,
) -> &'a serde_json::Map<String, serde_json::Value> {
    events
        .iter()
        .find_map(|entry| {
            let fields = entry.get("fields")?.as_object()?;
            (fields.get("event").and_then(serde_json::Value::as_str) == Some(event))
                .then_some(fields)
        })
        .unwrap_or_else(|| panic!("expected captured logs to include event {event:?}: {events:?}"))
}

pub(super) fn matching_event_fields<'a>(
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

pub(super) fn event_count(events: &[serde_json::Value], event: &str) -> usize {
    matching_event_fields(events, event).len()
}

pub(super) fn field_str<'a>(
    fields: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<&'a str> {
    fields.get(field).and_then(serde_json::Value::as_str)
}

pub(super) fn field_bool(
    fields: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Option<bool> {
    fields.get(field).and_then(serde_json::Value::as_bool)
}

pub(super) fn fixed_test_clock(
    instants: Vec<time::OffsetDateTime>,
) -> impl Fn() -> time::OffsetDateTime + Send + Sync + 'static {
    let remaining = Arc::new(Mutex::new(instants.into_iter()));
    move || {
        remaining
            .lock()
            .expect("test clock state should not be poisoned")
            .next()
            .expect("test clock should have another timestamp ready")
    }
}

pub(super) fn sibling_archives_for(active_log_file: &std::path::Path) -> Vec<PathBuf> {
    let parent = active_log_file
        .parent()
        .expect("active logfile should have a parent directory");
    let file_name = active_log_file
        .file_name()
        .and_then(|value| value.to_str())
        .expect("active logfile should have a UTF-8 filename");
    let archive_prefix = format!("{file_name}.");
    let mut archives = std::fs::read_dir(parent)
        .expect("archive directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path != active_log_file)
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with(&archive_prefix))
        })
        .collect::<Vec<_>>();
    archives.sort();
    archives
}
