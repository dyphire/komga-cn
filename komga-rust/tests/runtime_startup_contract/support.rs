use super::*;

pub(super) const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

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
    komga_rust::SearchIndexLifecycle::bootstrap(index_dir)
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
