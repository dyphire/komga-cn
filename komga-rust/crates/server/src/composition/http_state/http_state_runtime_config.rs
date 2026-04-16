use super::*;

pub(super) fn runtime_profile(config: &RuntimeConfig) -> RuntimeProfile {
    match config.runtime_profile {
        ConfigRuntimeProfile::SnapshotAligned => RuntimeProfile::SnapshotAligned,
        ConfigRuntimeProfile::LiveLocaldb => RuntimeProfile::LiveLocaldb,
    }
}

pub(super) fn transient_books_state_file(config: &RuntimeConfig) -> PathBuf {
    let root = config
        .config_dir
        .as_deref()
        .or_else(|| config.database_file.parent())
        .unwrap_or_else(|| Path::new("."));
    root.join("transient-books-state.json")
}

pub(super) fn load_transient_books_records(
    state_file: &Path,
) -> Result<HashMap<String, komga_interfaces::http::state::TransientBookRecord>, String> {
    if !state_file.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(state_file).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

pub(super) fn persist_transient_books_records(
    state_file: &Path,
    records: &HashMap<String, komga_interfaces::http::state::TransientBookRecord>,
) -> Result<(), String> {
    if let Some(parent) = state_file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(records).map_err(|error| error.to_string())?;
    std::fs::write(state_file, content).map_err(|error| error.to_string())
}
